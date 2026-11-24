#define _POSIX_C_SOURCE 200809L
#include "hush_ring.h"

#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <process.h>
#define hush_getpid _getpid
#else
#include <sys/mman.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>
#define hush_getpid getpid
#endif

typedef struct HushRingShared {
    uint32_t magic;
    uint32_t version;
    uint32_t sample_rate;
    uint32_t channels;
    uint32_t capacity;
    uint32_t frame_samples;
    _Atomic uint32_t algorithmic_latency_samples;
    _Atomic uint32_t flags;
    _Atomic uint64_t write_count;
    _Atomic uint64_t underruns;
    _Atomic uint64_t resyncs;
    uint32_t producer_pid;
    uint32_t reserved0;
    uint64_t reserved1;
    uint64_t reserved2;
    float samples[HUSH_RING_CAPACITY];
} HushRingShared;

struct HushRing {
#ifdef _WIN32
    HANDLE mapping;
#else
    int fd;
#endif
    size_t map_len;
    HushRingShared *shared;
};

struct HushRingReader {
    HushRing *ring;
    int anchored;
    double anchor_timestamp;
    uint64_t anchor_write;
    uint64_t last_write;
    int seq_started;
    uint64_t read_count;
};

static void hush_ring_zero_header(HushRingShared *s)
{
    memset(s, 0, sizeof(HushRingShared));
    s->magic = HUSH_RING_MAGIC;
    s->version = HUSH_RING_VERSION;
    s->sample_rate = 48000;
    s->channels = 1;
    s->capacity = HUSH_RING_CAPACITY;
    s->frame_samples = 480;
    s->producer_pid = (uint32_t)hush_getpid();
    atomic_store_explicit(&s->algorithmic_latency_samples, 0, memory_order_relaxed);
    atomic_store_explicit(&s->write_count, 0, memory_order_relaxed);
    atomic_store_explicit(&s->underruns, 0, memory_order_relaxed);
    atomic_store_explicit(&s->resyncs, 0, memory_order_relaxed);
    atomic_store_explicit(&s->flags, 0, memory_order_relaxed);
}

static int hush_ring_valid_name(const char *name)
{
    if (!name || name[0] != '/' || name[1] == '\0') {
        return 0;
    }
    return strlen(name) < 64;
}

#ifdef _WIN32
static void hush_ring_win_name(const char *name, char *out, size_t out_len)
{
    const char *bare = (name && name[0] == '/') ? name + 1 : (name ? name : "hush-mic");
    snprintf(out, out_len, "Local\\%s", bare);
}

static HushRing *hush_ring_map_handle(HANDLE mapping, int initialize)
{
    if (!mapping) {
        return NULL;
    }
    HushRing *ring = calloc(1, sizeof(*ring));
    if (!ring) {
        CloseHandle(mapping);
        return NULL;
    }
    ring->mapping = mapping;
    ring->map_len = sizeof(HushRingShared);
    void *mapped = MapViewOfFile(mapping, FILE_MAP_ALL_ACCESS, 0, 0, ring->map_len);
    if (!mapped) {
        CloseHandle(mapping);
        free(ring);
        return NULL;
    }
    ring->shared = mapped;
    if (initialize) {
        hush_ring_zero_header(ring->shared);
        atomic_store_explicit(&ring->shared->flags, HUSH_RING_FLAG_ACTIVE, memory_order_release);
    }
    return ring;
}
#else
static HushRing *hush_ring_wrap(int fd, int initialize)
{
    HushRing *ring = calloc(1, sizeof(*ring));
    if (!ring) {
        close(fd);
        return NULL;
    }
    ring->fd = fd;
    ring->map_len = sizeof(HushRingShared);
    if (ftruncate(fd, (off_t)ring->map_len) != 0) {
        close(fd);
        free(ring);
        return NULL;
    }
    void *mapped = mmap(NULL, ring->map_len, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (mapped == MAP_FAILED) {
        close(fd);
        free(ring);
        return NULL;
    }
    ring->shared = mapped;
    if (initialize) {
        hush_ring_zero_header(ring->shared);
        atomic_store_explicit(&ring->shared->flags, HUSH_RING_FLAG_ACTIVE, memory_order_release);
    }
    return ring;
}
#endif

HushRing *hush_ring_create_named(const char *name)
{
    if (!hush_ring_valid_name(name)) {
        name = HUSH_SHM_NAME;
    }
#ifdef _WIN32
    char winname[96];
    hush_ring_win_name(name, winname, sizeof winname);
    HANDLE mapping = CreateFileMappingA(INVALID_HANDLE_VALUE, NULL, PAGE_READWRITE, 0,
        (DWORD)sizeof(HushRingShared), winname);
    return hush_ring_map_handle(mapping, 1);
#else
    int fd = shm_open(name, O_CREAT | O_RDWR, 0666);
    if (fd < 0) {
        return NULL;
    }
    return hush_ring_wrap(fd, 1);
#endif
}

HushRing *hush_ring_create(void)
{
    return hush_ring_create_named(HUSH_SHM_NAME);
}

HushRing *hush_ring_attach_named(const char *name)
{
    if (!hush_ring_valid_name(name)) {
        return NULL;
    }
#ifdef _WIN32
    char winname[96];
    hush_ring_win_name(name, winname, sizeof winname);
    HANDLE mapping = OpenFileMappingA(FILE_MAP_ALL_ACCESS, FALSE, winname);
    HushRing *ring = hush_ring_map_handle(mapping, 0);
    if (ring && (ring->shared->magic != HUSH_RING_MAGIC || ring->shared->version != HUSH_RING_VERSION)) {
        hush_ring_destroy(ring);
        return NULL;
    }
    return ring;
#else
    int fd = shm_open(name, O_RDWR, 0666);
    if (fd < 0) {
        return NULL;
    }
    struct stat st;
    if (fstat(fd, &st) != 0 || (size_t)st.st_size < sizeof(HushRingShared)) {
        close(fd);
        return NULL;
    }
    HushRing *ring = calloc(1, sizeof(*ring));
    if (!ring) {
        close(fd);
        return NULL;
    }
    ring->fd = fd;
    ring->map_len = sizeof(HushRingShared);
    void *mapped = mmap(NULL, ring->map_len, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (mapped == MAP_FAILED) {
        close(fd);
        free(ring);
        return NULL;
    }
    ring->shared = mapped;
    if (ring->shared->magic != HUSH_RING_MAGIC || ring->shared->version != HUSH_RING_VERSION) {
        hush_ring_destroy(ring);
        return NULL;
    }
    return ring;
#endif
}

HushRing *hush_ring_attach(void)
{
    return hush_ring_attach_named(HUSH_SHM_NAME);
}

void hush_ring_destroy(HushRing *ring)
{
    if (!ring) {
        return;
    }
    if (ring->shared) {
#ifdef _WIN32
        UnmapViewOfFile(ring->shared);
#else
        munmap(ring->shared, ring->map_len);
#endif
    }
#ifdef _WIN32
    if (ring->mapping) {
        CloseHandle(ring->mapping);
    }
#else
    if (ring->fd >= 0) {
        close(ring->fd);
    }
#endif
    free(ring);
}

void hush_ring_set_active(HushRing *ring, int active)
{
    if (!ring || !ring->shared) {
        return;
    }
    uint32_t flags = atomic_load_explicit(&ring->shared->flags, memory_order_relaxed);
    if (active) {
        flags |= HUSH_RING_FLAG_ACTIVE;
    } else {
        flags &= ~HUSH_RING_FLAG_ACTIVE;
    }
    atomic_store_explicit(&ring->shared->flags, flags, memory_order_release);
}

void hush_ring_set_muted(HushRing *ring, int muted)
{
    if (!ring || !ring->shared) {
        return;
    }
    uint32_t flags = atomic_load_explicit(&ring->shared->flags, memory_order_relaxed);
    if (muted) {
        flags |= HUSH_RING_FLAG_MUTED;
    } else {
        flags &= ~HUSH_RING_FLAG_MUTED;
    }
    atomic_store_explicit(&ring->shared->flags, flags, memory_order_release);
}

void hush_ring_set_latency(HushRing *ring, uint32_t samples)
{
    if (!ring || !ring->shared) {
        return;
    }
    atomic_store_explicit(&ring->shared->algorithmic_latency_samples, samples, memory_order_relaxed);
}

uint32_t hush_ring_latency(const HushRing *ring)
{
    if (!ring || !ring->shared) {
        return 0;
    }
    return atomic_load_explicit(&ring->shared->algorithmic_latency_samples, memory_order_relaxed);
}

uint32_t hush_ring_write(HushRing *ring, const float *src, uint32_t n)
{
    if (!ring || !ring->shared || !src || n == 0) {
        return 0;
    }
    HushRingShared *s = ring->shared;
    uint64_t start = atomic_load_explicit(&s->write_count, memory_order_relaxed);
    uint32_t cap = s->capacity;
    if (cap == 0) {
        return 0;
    }
    for (uint32_t i = 0; i < n; i++) {
        s->samples[(start + i) % cap] = src[i];
    }
    atomic_store_explicit(&s->write_count, start + n, memory_order_release);
    return n;
}

uint64_t hush_ring_underruns(const HushRing *ring)
{
    if (!ring || !ring->shared) {
        return 0;
    }
    return atomic_load_explicit(&ring->shared->underruns, memory_order_relaxed);
}

uint64_t hush_ring_resyncs(const HushRing *ring)
{
    if (!ring || !ring->shared) {
        return 0;
    }
    return atomic_load_explicit(&ring->shared->resyncs, memory_order_relaxed);
}

uint64_t hush_ring_write_count(const HushRing *ring)
{
    if (!ring || !ring->shared) {
        return 0;
    }
    return atomic_load_explicit(&ring->shared->write_count, memory_order_acquire);
}

int hush_ring_is_active(const HushRing *ring)
{
    if (!ring || !ring->shared) {
        return 0;
    }
    uint32_t flags = atomic_load_explicit(&ring->shared->flags, memory_order_acquire);
    return (flags & HUSH_RING_FLAG_ACTIVE) != 0;
}

const char *hush_ring_shm_name(void)
{
    return HUSH_SHM_NAME;
}

size_t hush_ring_shared_bytes(void)
{
    return sizeof(HushRingShared);
}

HushRingReader *hush_ring_reader_new(HushRing *ring)
{
    HushRingReader *reader = calloc(1, sizeof(*reader));
    if (!reader) {
        return NULL;
    }
    reader->ring = ring;
    return reader;
}

void hush_ring_reader_free(HushRingReader *reader)
{
    free(reader);
}

void hush_ring_reader_reset(HushRingReader *reader)
{
    if (!reader) {
        return;
    }
    reader->anchored = 0;
    reader->anchor_timestamp = 0;
    reader->anchor_write = 0;
    reader->last_write = 0;
    reader->seq_started = 0;
    reader->read_count = 0;
}

uint32_t hush_ring_read(HushRingReader *reader, double timestamp_frames, float *dst, uint32_t n)
{
    if (!reader || !dst || n == 0) {
        return 0;
    }
    for (uint32_t i = 0; i < n; i++) {
        dst[i] = 0.f;
    }
    HushRing *ring = reader->ring;
    if (!ring || !ring->shared) {
        return n;
    }
    HushRingShared *s = ring->shared;
    uint32_t flags = atomic_load_explicit(&s->flags, memory_order_acquire);
    if (s->magic != HUSH_RING_MAGIC || s->version != HUSH_RING_VERSION
        || (flags & HUSH_RING_FLAG_ACTIVE) == 0) {
        reader->anchored = 0;
        return n;
    }

    uint64_t write = atomic_load_explicit(&s->write_count, memory_order_acquire);
    if (reader->anchored && write < reader->last_write) {
        reader->anchored = 0;
    }
    reader->last_write = write;

    uint32_t cap = s->capacity;
    uint32_t rate = s->sample_rate;
    if (cap == 0 || rate == 0) {
        return n;
    }

    if ((flags & HUSH_RING_FLAG_MUTED) != 0) {
        reader->anchored = 0;
        return n;
    }
    uint64_t jitter = (uint64_t)rate / 50ull; /* 20 ms */
    if (jitter > write) {
        jitter = write;
    }
    if (!reader->anchored) {
        reader->anchored = 1;
        reader->anchor_timestamp = timestamp_frames;
        reader->anchor_write = write - jitter;
    }

    if (timestamp_frames < reader->anchor_timestamp) {
        return n;
    }
    uint64_t delta = (uint64_t)(timestamp_frames - reader->anchor_timestamp + 0.5);
    uint64_t start = reader->anchor_write + delta;
    /* Hardware mic clock and the virtual-device clock drift. Jump only the
       excess over a 40 ms cushion, and only once the lag passes 80 ms, so a
       slow walk does not become a quarter-second skip. */
    uint64_t max_lag = (uint64_t)rate / 12ull; /* ~83 ms */
    if (write > start && (write - start) > max_lag + (uint64_t)n) {
        uint64_t lag = write - start;
        uint64_t cushion = (uint64_t)rate / 25ull; /* 40 ms */
        uint64_t skip = lag > cushion ? lag - cushion : 0;
        reader->anchor_write += skip;
        start += skip;
        atomic_fetch_add_explicit(&s->resyncs, 1, memory_order_relaxed);
    }

    uint32_t underruns = 0;
    for (uint32_t i = 0; i < n; i++) {
        uint64_t pos = start + (uint64_t)i;
        if (pos < write && (write - pos) <= (uint64_t)cap) {
            dst[i] = s->samples[pos % cap];
        } else {
            underruns++;
        }
    }
    if (underruns) {
        atomic_fetch_add_explicit(&s->underruns, underruns, memory_order_relaxed);
    }
    return underruns;
}

uint32_t hush_ring_read_next(HushRingReader *reader, float *dst, uint32_t n)
{
    if (!reader || !dst || n == 0) {
        return 0;
    }
    for (uint32_t i = 0; i < n; i++) {
        dst[i] = 0.f;
    }
    HushRing *ring = reader->ring;
    if (!ring || !ring->shared) {
        return n;
    }
    HushRingShared *s = ring->shared;
    uint32_t flags = atomic_load_explicit(&s->flags, memory_order_acquire);
    if (s->magic != HUSH_RING_MAGIC || (flags & HUSH_RING_FLAG_ACTIVE) == 0
        || (flags & HUSH_RING_FLAG_MUTED) != 0) {
        reader->seq_started = 0;
        return n;
    }
    uint64_t write = atomic_load_explicit(&s->write_count, memory_order_acquire);
    uint32_t cap = s->capacity;
    uint32_t rate = s->sample_rate == 0 ? 48000u : s->sample_rate;
    if (cap == 0) {
        return n;
    }
    uint64_t jitter = (uint64_t)rate / 50ull;
    if (!reader->seq_started || write < reader->read_count) {
        reader->seq_started = 1;
        reader->read_count = write > jitter ? write - jitter : 0;
    }
    uint64_t max_lag = (uint64_t)rate / 12ull;
    if (write > reader->read_count && (write - reader->read_count) > max_lag + n) {
        uint64_t lag = write - reader->read_count;
        uint64_t cushion = (uint64_t)rate / 25ull;
        if (lag > cushion) {
            reader->read_count += lag - cushion;
        }
        atomic_fetch_add_explicit(&s->resyncs, 1, memory_order_relaxed);
    }
    uint32_t underruns = 0;
    for (uint32_t i = 0; i < n; i++) {
        uint64_t pos = reader->read_count;
        if (pos < write && (write - pos) <= (uint64_t)cap) {
            dst[i] = s->samples[pos % cap];
            reader->read_count++;
        } else {
            underruns++;
        }
    }
    if (underruns) {
        atomic_fetch_add_explicit(&s->underruns, underruns, memory_order_relaxed);
    }
    return underruns;
}
