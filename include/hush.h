#ifndef HUSH_H
#define HUSH_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Single-threaded C API for the Hush 48 kHz pipeline.
 * One handle must not be used from two threads at once.
 *
 * ns_backend: 0 off, 1 DeepFilterNet3, 2 RNNoise, 3 DPDFNet8.
 * strength: 0 skips noise suppression, 1 is the model's full attenuation.
 * Frames are mono float in [-1, 1], 480 samples (10 ms) at 48 kHz.
 * far may be NULL when echo cancellation has no reference.
 */

enum {
    HUSH_NS_OFF = 0,
    HUSH_NS_DEEPFILTER = 1,
    HUSH_NS_RNNOISE = 2,
    HUSH_NS_DPDFNET = 3
};

enum {
    HUSH_OK = 0,
    HUSH_ERR_INVALID = -1,
    HUSH_ERR_FAILED = -2,
    HUSH_ERR_PANIC = -3
};

typedef struct HushConfig {
    int enabled;
    int aec_enabled;
    int ns_backend;
    float strength;
} HushConfig;

typedef struct HushFrameStats {
    uint64_t process_ns;
    float cpu_percent;
    uint32_t algorithmic_latency_samples;
    int aec_applied;
    int ns_applied;
} HushFrameStats;

typedef struct HushPipeline HushPipeline;

uint32_t hush_sample_rate(void);
uint32_t hush_frame_samples(void);

HushPipeline *hush_pipeline_create(const HushConfig *config, char *err, size_t err_len);
void hush_pipeline_destroy(HushPipeline *pipeline);
int hush_pipeline_set_config(HushPipeline *pipeline, const HushConfig *config, char *err, size_t err_len);
uint32_t hush_pipeline_latency_samples(const HushPipeline *pipeline);
int hush_pipeline_process(
    HushPipeline *pipeline,
    const float *near,
    const float *far,
    float *out,
    HushFrameStats *stats);

#ifdef __cplusplus
}
#endif

#endif
