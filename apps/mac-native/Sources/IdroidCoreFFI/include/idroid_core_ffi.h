#ifndef IDROID_CORE_FFI_H
#define IDROID_CORE_FFI_H

#include <stdbool.h>
#include <stdint.h>

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

#endif
