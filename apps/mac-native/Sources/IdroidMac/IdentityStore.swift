import CryptoKit
import Foundation
import Security

struct MacIdentity {
    static let keychainLabel = "com.idroid.mac-native.identity"

    let identity: SecIdentity
    let certificate: SecCertificate
    let fingerprint: String

    init() throws {
        if let identity = Self.loadIdentity() {
            self = try Self.make(identity: identity)
            return
        }

        let identity = try Self.generateAndImportIdentity()
        self = try Self.make(identity: identity)
    }

    private static func loadIdentity() -> SecIdentity? {
        var result: CFTypeRef?
        let query: [String: Any] = [
            kSecClass as String: kSecClassIdentity,
            kSecAttrLabel as String: keychainLabel,
            kSecReturnRef as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess,
              let result,
              CFGetTypeID(result) == SecIdentityGetTypeID() else { return nil }
        return (result as! SecIdentity)
    }

    private static func make(identity: SecIdentity) throws -> MacIdentity {
        var certificate: SecCertificate?
        guard SecIdentityCopyCertificate(identity, &certificate) == errSecSuccess,
              let certificate else {
            throw IdentityError.certificateUnavailable
        }
        let bytes = SecCertificateCopyData(certificate) as Data
        let fingerprint = SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
        return MacIdentity(identity: identity, certificate: certificate, fingerprint: fingerprint)
    }

    private init(identity: SecIdentity, certificate: SecCertificate, fingerprint: String) {
        self.identity = identity
        self.certificate = certificate
        self.fingerprint = fingerprint
    }

    private static func generateAndImportIdentity() throws -> SecIdentity {
        // Clean up any stale keys or certificates with this label before generating a new pair
        let delKey: [String: Any] = [
            kSecClass as String: kSecClassKey,
            kSecAttrLabel as String: keychainLabel,
        ]
        SecItemDelete(delKey as CFDictionary)
        let delCert: [String: Any] = [
            kSecClass as String: kSecClassCertificate,
            kSecAttrLabel as String: keychainLabel,
        ]
        SecItemDelete(delCert as CFDictionary)

        // 1. Generate RSA 2048 key directly in Keychain
        let keyParams: [String: Any] = [
            kSecAttrKeyType as String: kSecAttrKeyTypeRSA,
            kSecAttrKeySizeInBits as String: 2048,
            kSecPrivateKeyAttrs as String: [
                kSecAttrIsPermanent as String: true,
                kSecAttrLabel as String: keychainLabel,
                kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
            ],
        ]
        var error: Unmanaged<CFError>?
        guard let privateKey = SecKeyCreateRandomKey(keyParams as CFDictionary, &error) else {
            throw IdentityError.keyGenerationFailed
        }
        guard let publicKey = SecKeyCopyPublicKey(privateKey),
              let pubKeyData = SecKeyCopyExternalRepresentation(publicKey, &error) as Data? else {
            throw IdentityError.keyGenerationFailed
        }

        // 2. Build self-signed X.509 certificate (RFC 5280)
        let sha256WithRSA = ASN1.sequence([
            ASN1.objectIdentifier([0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0B]),
            ASN1.null(),
        ])
        let rsaEncryption = ASN1.sequence([
            ASN1.objectIdentifier([0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01]),
            ASN1.null(),
        ])
        let spki = ASN1.sequence([
            rsaEncryption,
            ASN1.bitString(pubKeyData),
        ])
        let name = ASN1.sequence([
            ASN1.set([
                ASN1.sequence([
                    ASN1.objectIdentifier([0x55, 0x04, 0x03]),
                    ASN1.utf8String("iDroid Mac"),
                ]),
            ]),
        ])
        let serial = ASN1.integer(Data((0..<16).map { _ in UInt8.random(in: 0...255) }))
        let now = Date()
        let validity = ASN1.sequence([
            ASN1.utcTime(now),
            ASN1.utcTime(now.addingTimeInterval(3650 * 86400)), // 10 years
        ])
        let version = ASN1.explicit(tag: 0, ASN1.integer(Data([0x02]))) // v3

        let tbs = ASN1.sequence([
            version,
            serial,
            sha256WithRSA,
            name,
            validity,
            name,
            spki,
        ])

        guard let signature = SecKeyCreateSignature(
            privateKey,
            .rsaSignatureMessagePKCS1v15SHA256,
            tbs as CFData,
            &error
        ) as Data? else {
            throw IdentityError.signatureFailed
        }

        let certData = ASN1.sequence([
            tbs,
            sha256WithRSA,
            ASN1.bitString(signature),
        ])

        guard let certificate = SecCertificateCreateWithData(nil, certData as CFData) else {
            throw IdentityError.certificateCreationFailed
        }

        // 3. Store certificate in Keychain with matching label
        let certItem: [String: Any] = [
            kSecClass as String: kSecClassCertificate,
            kSecValueRef as String: certificate,
            kSecAttrLabel as String: keychainLabel,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        let status = SecItemAdd(certItem as CFDictionary, nil)
        guard status == errSecSuccess || status == errSecDuplicateItem else {
            throw IdentityError.keychainWriteFailed(status)
        }

        if let identity = loadIdentity() {
            return identity
        }

        guard let directIdentity = SecIdentityCreate(nil, certificate, privateKey) else {
            throw IdentityError.identityCreationFailed
        }
        return directIdentity
    }
}

private enum ASN1 {
    static func sequence(_ elements: [Data]) -> Data {
        let payload = elements.reduce(Data(), +)
        return wrap(tag: 0x30, payload: payload)
    }

    static func set(_ elements: [Data]) -> Data {
        let payload = elements.reduce(Data(), +)
        return wrap(tag: 0x31, payload: payload)
    }

    static func integer(_ value: Data) -> Data {
        var val = value
        if let first = val.first, first >= 0x80 {
            val.insert(0x00, at: 0)
        }
        return wrap(tag: 0x02, payload: val)
    }

    static func bitString(_ data: Data) -> Data {
        var payload = Data([0x00])
        payload.append(data)
        return wrap(tag: 0x03, payload: payload)
    }

    static func null() -> Data {
        return Data([0x05, 0x00])
    }

    static func objectIdentifier(_ oid: [UInt8]) -> Data {
        return wrap(tag: 0x06, payload: Data(oid))
    }

    static func utf8String(_ string: String) -> Data {
        return wrap(tag: 0x0C, payload: Data(string.utf8))
    }

    static func utcTime(_ date: Date) -> Data {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyMMddHHmmss'Z'"
        formatter.timeZone = TimeZone(secondsFromGMT: 0)
        formatter.locale = Locale(identifier: "en_US_POSIX")
        let str = formatter.string(from: date)
        return wrap(tag: 0x17, payload: Data(str.utf8))
    }

    static func explicit(tag: UInt8, _ payload: Data) -> Data {
        return wrap(tag: 0xA0 | tag, payload: payload)
    }

    static func wrap(tag: UInt8, payload: Data) -> Data {
        var result = Data([tag])
        let length = payload.count
        if length < 0x80 {
            result.append(UInt8(length))
        } else if length <= 0xFF {
            result.append(contentsOf: [0x81, UInt8(length)])
        } else if length <= 0xFFFF {
            result.append(contentsOf: [0x82, UInt8(length >> 8), UInt8(length & 0xFF)])
        } else {
            result.append(contentsOf: [
                0x84,
                UInt8((length >> 24) & 0xFF),
                UInt8((length >> 16) & 0xFF),
                UInt8((length >> 8) & 0xFF),
                UInt8(length & 0xFF),
            ])
        }
        result.append(payload)
        return result
    }
}

enum IdentityError: LocalizedError {
    case certificateUnavailable
    case keyGenerationFailed
    case signatureFailed
    case certificateCreationFailed
    case identityCreationFailed
    case keychainWriteFailed(OSStatus)

    var errorDescription: String? {
        switch self {
        case .certificateUnavailable: return "Mac certificate is unavailable"
        case .keyGenerationFailed: return "Mac key generation failed"
        case .signatureFailed: return "Mac certificate signing failed"
        case .certificateCreationFailed: return "Mac certificate creation failed"
        case .identityCreationFailed: return "Mac identity creation failed"
        case .keychainWriteFailed(let status): return "Mac Keychain write failed (\(status))"
        }
    }
}

struct PairedMacPeer: Codable {
    let fingerprint: String
    let name: String
    let certificateBase64: String
    let pairedAt: Int64
}

final class MacTrustStore {
    private let service = "com.idroid.mac-native.peer"
    private let account = "android"

    func load() -> PairedMacPeer? {
        var result: CFTypeRef?
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess,
              let data = result as? Data else { return nil }
        return try? JSONDecoder().decode(PairedMacPeer.self, from: data)
    }

    func save(_ peer: PairedMacPeer) throws {
        let data = try JSONEncoder().encode(peer)
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        let values: [String: Any] = [kSecValueData as String: data]
        let status = SecItemUpdate(query as CFDictionary, values as CFDictionary)
        if status == errSecItemNotFound {
            var item = query
            item[kSecValueData as String] = data
            guard SecItemAdd(item as CFDictionary, nil) == errSecSuccess else {
                throw IdentityError.keychainWriteFailed(status)
            }
        } else if status != errSecSuccess {
            throw IdentityError.keychainWriteFailed(status)
        }
    }

    func forget() throws {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        let status = SecItemDelete(query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw IdentityError.keychainWriteFailed(status)
        }
    }
}
