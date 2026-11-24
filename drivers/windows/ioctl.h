#ifndef HUSH_WINDOWS_IOCTL_H
#define HUSH_WINDOWS_IOCTL_H

/*
 * User-mode service <-> virtual audio driver.
 *
 * Prefer the named file mappings in native/ring/ring.c when the driver can
 * open them. Use these IOCTLs when it cannot (session 0 versus the user's
 * Local\ namespace). The payload is tightly packed mono float32 at 48 kHz.
 *
 * Device type 0x8000 is in the vendor range. METHOD_BUFFERED copies the
 * buffer so the driver does not touch a user pointer directly.
 */

#if defined(_NTDDK_) || defined(_WDMDDK_)
/* Kernel builds already have CTL_CODE from devioctl.h. ULONG is 32-bit. */
typedef ULONG hush_u32;
#elif defined(_WIN32)
#include <stdint.h>
#include <winioctl.h>
typedef uint32_t hush_u32;
#else
#include <stdint.h>
#define FILE_DEVICE_UNKNOWN 0x00000022
#define METHOD_BUFFERED 0
#define FILE_ANY_ACCESS 0
#define CTL_CODE(DeviceType, Function, Method, Access) \
    (((DeviceType) << 16) | ((Access) << 14) | ((Function) << 2) | (Method))
typedef uint32_t hush_u32;
#endif

#define HUSH_DEVICE_TYPE 0x8000

#define HUSH_IOCTL_WRITE_CAPTURE \
    CTL_CODE(HUSH_DEVICE_TYPE, 0x800, METHOD_BUFFERED, FILE_ANY_ACCESS)
#define HUSH_IOCTL_READ_RENDER \
    CTL_CODE(HUSH_DEVICE_TYPE, 0x801, METHOD_BUFFERED, FILE_ANY_ACCESS)

/* 12 byte header + 480 mono floats. Must match HushIoctlAudio in winbridge.rs. */
typedef struct HushIoctlAudio {
    hush_u32 sample_rate;
    hush_u32 channels;
    hush_u32 frames;
    float samples[480];
} HushIoctlAudio;

#endif
