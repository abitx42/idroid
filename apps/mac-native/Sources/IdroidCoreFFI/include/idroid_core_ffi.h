#ifndef IDROID_CORE_FFI_H
#define IDROID_CORE_FFI_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct IdroidLink IdroidLink;

typedef enum IdroidLinkState {
    IDROID_LINK_SEARCHING = 0,
    IDROID_LINK_CONNECTING = 1,
    IDROID_LINK_HANDSHAKING = 2,
    IDROID_LINK_READY = 3,
    IDROID_LINK_DEGRADED = 4,
    IDROID_LINK_LOST = 5,
} IdroidLinkState;

typedef enum IdroidLinkResult {
    IDROID_LINK_OK = 0,
    IDROID_LINK_INVALID_HANDLE = 1,
    IDROID_LINK_INVALID_TRANSITION = 2,
    IDROID_LINK_INVALID_SESSION = 3,
    IDROID_LINK_SESSION_CHANGED = 4,
    IDROID_LINK_CLOCK_REWOUND = 5,
} IdroidLinkResult;

uint32_t idroid_core_ffi_version(void);
IdroidLink *idroid_link_new(uint64_t now_ms);
void idroid_link_free(IdroidLink *link);
IdroidLinkState idroid_link_state(const IdroidLink *link);
IdroidLinkResult idroid_link_begin_connecting(IdroidLink *link, uint64_t now_ms);
IdroidLinkResult idroid_link_transport_connected(IdroidLink *link, uint64_t now_ms);
IdroidLinkResult idroid_link_connection_failed(IdroidLink *link, uint64_t now_ms);
IdroidLinkResult idroid_link_network_changed(IdroidLink *link, uint64_t now_ms);
IdroidLinkResult idroid_link_handshake_ready(
    IdroidLink *link,
    uint64_t now_ms,
    bool has_session,
    uint64_t session_id
);
IdroidLinkResult idroid_link_reconnect_time_ms(
    const IdroidLink *link,
    uint64_t now_ms,
    uint64_t *value
);

/* Binary codec. Result codes are negative on failure. */

#define IDROID_CODEC_OK 0
#define IDROID_CODEC_INVALID_FRAME (-1)
#define IDROID_CODEC_BUFFER_TOO_SMALL (-2)
#define IDROID_CODEC_INVALID_ARGUMENT (-3)
#define IDROID_CODEC_WRONG_KIND (-4)

/* Wire values for IdroidHelloView.role. */
#define IDROID_PEER_ROLE_ANDROID 1
#define IDROID_PEER_ROLE_MAC 2

typedef struct IdroidHelloView {
    uint16_t version_min;
    uint16_t version_max;
    uint8_t fingerprint[32];
    uint8_t role;
    uint16_t app_version_len;
    uint8_t app_version[64];
    uint8_t has_last_session;
    uint64_t last_session_id;
} IdroidHelloView;

typedef struct IdroidByeView {
    uint8_t has_code;
    uint8_t code;
    uint8_t retryable;
    uint8_t has_user_action;
    uint8_t user_action;
} IdroidByeView;

int32_t idroid_codec_encode_hello(
    uint16_t version_min,
    uint16_t version_max,
    const uint8_t *fingerprint,
    uint8_t role,
    const uint8_t *app_version,
    size_t app_version_len,
    bool has_last_session,
    uint64_t last_session_id,
    uint8_t *out,
    size_t out_capacity,
    size_t *out_len
);
int32_t idroid_codec_decode_hello(
    const uint8_t *frame,
    size_t frame_len,
    IdroidHelloView *out
);
int32_t idroid_codec_encode_hello_ack(
    uint16_t version,
    uint8_t *out,
    size_t out_capacity,
    size_t *out_len
);
int32_t idroid_codec_decode_hello_ack(
    const uint8_t *frame,
    size_t frame_len,
    uint16_t *version
);
int32_t idroid_codec_encode_bye(
    bool has_code,
    uint8_t code,
    bool retryable,
    bool has_user_action,
    uint8_t user_action,
    uint8_t *out,
    size_t out_capacity,
    size_t *out_len
);
int32_t idroid_codec_decode_bye(
    const uint8_t *frame,
    size_t frame_len,
    IdroidByeView *out
);
int32_t idroid_codec_frame_kind(const uint8_t *frame, size_t frame_len);
int32_t idroid_codec_frame_version(const uint8_t *frame, size_t frame_len);
int32_t idroid_codec_validate(const uint8_t *frame, size_t frame_len);

#ifdef __cplusplus
}
#endif

#endif
