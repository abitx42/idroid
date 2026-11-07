#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

static uint64_t nanos(clockid_t clock_id) {
    struct timespec value;
    if (clock_gettime(clock_id, &value) != 0) {
        perror("clock_gettime");
        exit(EXIT_FAILURE);
    }
    return (uint64_t)value.tv_sec * UINT64_C(1000000000) + (uint64_t)value.tv_nsec;
}

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: %s LABEL\n", argv[0]);
        return EXIT_FAILURE;
    }
    printf("%s monotonic_ns=%llu uptime_raw_ns=%llu\n",
           argv[1],
           (unsigned long long)nanos(CLOCK_MONOTONIC),
           (unsigned long long)nanos(CLOCK_UPTIME_RAW));
    return EXIT_SUCCESS;
}
