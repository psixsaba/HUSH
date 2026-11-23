#pragma once

/*
 * User-mode helper <-> miniport queues.
 *
 * Capture: the helper pushes 48 kHz mono float, WriteBytes pulls it into the
 * microphone DMA buffer. Render: ReadBytes pushes what the call app played,
 * the helper pulls it into Krisp Alt Speaker.
 *
 * The IOCTL codes live in drivers/windows/ioctl.h and must stay in lockstep
 * with crates/hush-core/src/winbridge.rs.
 */

#include "definitions.h"

#define HUSH_BRIDGE_SAMPLES 480

EXTERN_C_START

NTSTATUS HushInstallBridge(_In_ PDRIVER_OBJECT DriverObject);
void HushRemoveBridge(void);

/* FALSE when the queue was empty. *Sample is 0 in that case. Non-paged. */
BOOLEAN HushCapturePop(_Out_ float* Sample);
void HushRenderPush(_In_reads_(Count) const float* Samples, _In_ ULONG Count);

EXTERN_C_END
