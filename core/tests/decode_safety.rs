//! Decoder safety properties that do not need a nightly toolchain.
//!
//! A real `cargo-fuzz` target still belongs here eventually (A-018), but these
//! deterministic properties already run in `make test`: no input may panic, an
//! accepted frame must be canonical (re-encoding it reproduces the same bytes),
//! trailing bytes must be rejected, and incremental framing must agree with
//! one-shot decoding.

use idroid_core::{FrameReader, ProtocolMessage};

const FIXTURE: &str = include_str!("../../tests/protocol/control-plane-v2.txt");

fn vectors() -> Vec<Vec<u8>> {
    FIXTURE
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (_, hex) = line.split_once(' ').expect("name and hex");
            (0..hex.len())
                .step_by(2)
                .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).expect("valid hex"))
                .collect()
        })
        .collect()
}

/// Small deterministic PRNG so failures are reproducible without a dependency.
struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            (self.next() % bound as u64) as usize
        }
    }
}

fn assert_canonical(bytes: &[u8]) {
    if let Ok(message) = ProtocolMessage::decode(bytes) {
        let reencoded = message.encode().expect("an accepted frame re-encodes");
        assert_eq!(
            reencoded, bytes,
            "decoder accepted bytes the encoder would not produce"
        );
        let mut trailing = bytes.to_vec();
        trailing.push(0);
        assert!(
            ProtocolMessage::decode(&trailing).is_err(),
            "trailing bytes were accepted"
        );
    }
}

#[test]
fn single_byte_mutations_never_panic_and_stay_canonical() {
    let replacements = [0x00u8, 0x01, 0x02, 0x7f, 0x80, 0xfe, 0xff];
    for frame in vectors() {
        for index in 0..frame.len() {
            for replacement in replacements {
                let mut mutated = frame.clone();
                mutated[index] = replacement;
                assert_canonical(&mutated);

                let mut reader = FrameReader::new();
                assert!(reader.push(&mutated).is_ok());
                if let Ok(Some(message)) = reader.next_frame() {
                    assert_eq!(message.encode().expect("encodes"), mutated);
                }
            }
        }
    }
}

#[test]
fn random_inputs_never_panic() {
    let mut rng = XorShift(0x1d20_1d20_5eed_1234);
    for _ in 0..20_000 {
        let len = rng.below(512);
        let mut bytes = vec![0u8; len];
        for byte in &mut bytes {
            *byte = rng.next() as u8;
        }
        assert_canonical(&bytes);

        let mut reader = FrameReader::new();
        if reader.push(&bytes).is_ok() {
            if let Ok(Some(message)) = reader.next_frame() {
                assert!(message.encode().is_ok());
            }
        }
    }
}

#[test]
fn valid_frames_survive_arbitrary_chunking() {
    let mut rng = XorShift(0xdead_beef_0bad_f00d);
    for frame in vectors() {
        for _ in 0..25 {
            let expected = ProtocolMessage::decode(&frame).expect("fixture decodes");
            let mut reader = FrameReader::new();
            let mut offset = 0;
            let mut delivered = None;
            while offset < frame.len() {
                let chunk = 1 + rng.below(frame.len() - offset);
                reader
                    .push(&frame[offset..offset + chunk])
                    .expect("push within bounds");
                offset += chunk;
                if delivered.is_none() {
                    delivered = reader.next_frame().expect("valid frame");
                }
            }
            assert_eq!(
                delivered,
                Some(expected),
                "chunked frame did not round-trip"
            );
        }
    }
}

#[test]
fn stalled_streams_are_bounded() {
    let mut reader = FrameReader::new();
    let header = [b'I', b'D', b'R', b'0', 2, 17, 0xff, 0xff, 0xff, 0x7f];
    assert!(reader.push(&header).is_ok());
    assert!(matches!(
        reader.next_frame(),
        Err(idroid_core::ProtocolError::PayloadTooLarge { .. })
    ));

    let mut stalled = FrameReader::new();
    assert!(matches!(
        stalled.push(&vec![0u8; idroid_core::MAX_FRAME_SIZE * 2 + 1]),
        Err(idroid_core::ProtocolError::PayloadTooLarge { .. })
    ));
}
