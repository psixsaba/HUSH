/*
 * Shared-memory ring for the Hush virtual microphone.
 *
 * The producer (the desktop app or `hush run`) writes mono 48 kHz float
 * samples. The Core Audio plugin, and any other consumer, reads them by
 * device timestamp. The ring is a broadcast buffer: reading does not consume,
 * so every client of the virtual mic hears the same signal.
 *
 * Protocol:
 *   1. Publisher maps the segment read/write, fills the header, then
 *      release-stores HUSH_RING_FLAG_ACTIVE.
 *   2. Sample stores happen before a release store of write_count.
 *   3. Readers acquire-load flags and write_count, then read samples with
 *      index < write_count and not older than `capacity`.
 *   4. Dropping the publisher release-stores flags without ACTIVE. The
 *      segment is left in place so an already-mapped plugin observes the
 *      flag instead of a dangling name.
 *
 * The segment is mode 0666. A HAL plugin and the user app do not share a
 * uid on every macOS configuration, and the reader also increments the
 * underrun counter. Treat the segment as local-machine audio, not a secret.
 */
#ifndef HUSH_RING_H
#define HUSH_RING_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define HUSH_RING_MAGIC 0x48555331u /* 'HUS1' */
#define HUSH_RING_VERSION 1u
#define HUSH_RING_CAPACITY 65536u
#define HUSH_SHM_NAME "/hush-mic"
#define HUSH_SHM_SPEAKER "/hush-spk"
#define HUSH_RING_FLAG_ACTIVE 1u
#define HUSH_RING_FLAG_MUTED 2u

typedef struct HushRing HushRing;
typedef struct HushRingReader HushRingReader;

/* Create or reset a segment and mark it active. NULL on failure.
   `name` is a POSIX shm name such as HUSH_SHM_NAME. NULL uses the mic. */
HushRing *hush_ring_create_named(const char *name);
HushRing *hush_ring_create(void);

/* Map an existing segment. NULL if the publisher has not created it. */
HushRing *hush_ring_attach_named(const char *name);
HushRing *hush_ring_attach(void);

void hush_ring_destroy(HushRing *ring);

/* Remove the name. Mappings already held by this process or the plugin stay
   valid until they are destroyed. Returns 0, or -1 on error. ENOENT is success. */
int hush_ring_unlink(const char *name);

/* Empty string after success. After a NULL create/attach, a short reason
   including errno ("shared memory sizing: Invalid argument"). */
const char *hush_ring_last_error(void);

void hush_ring_set_active(HushRing *ring, int active);
void hush_ring_set_muted(HushRing *ring, int muted);
void hush_ring_set_latency(HushRing *ring, uint32_t samples);
uint32_t hush_ring_latency(const HushRing *ring);
uint32_t hush_ring_write(HushRing *ring, const float *src, uint32_t n);

uint64_t hush_ring_underruns(const HushRing *ring);
uint64_t hush_ring_resyncs(const HushRing *ring);
uint64_t hush_ring_write_count(const HushRing *ring);
int hush_ring_is_active(const HushRing *ring);

const char *hush_ring_shm_name(void);
size_t hush_ring_shared_bytes(void);

HushRingReader *hush_ring_reader_new(HushRing *ring);
void hush_ring_reader_free(HushRingReader *reader);
void hush_ring_reader_reset(HushRingReader *reader);

/*
 * Copy `n` mono samples for a device timestamp measured in frames.
 * Returns how many of those samples were silence because the publisher
 * had not reached them yet.
 */
uint32_t hush_ring_read(HushRingReader *reader, double timestamp_frames, float *dst, uint32_t n);

/* Sequential read for the virtual-speaker bridge. Advances a private cursor. */
uint32_t hush_ring_read_next(HushRingReader *reader, float *dst, uint32_t n);

#ifdef __cplusplus
}
#endif

#endif
