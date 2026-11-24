#include "hush_bridge.h"
#include "../../../ioctl.h"

C_ASSERT(sizeof(HushIoctlAudio) == 12 + (480 * sizeof(float)));

#include <wdmsec.h>

#pragma comment(lib, "wdmsec.lib")

#define HUSH_QUEUE_SAMPLES (480 * 64)
#define HUSH_BRIDGE_POOLTAG 'hsuH'

static const GUID HUSH_CONTROL_CLASS =
{ 0x8a1c4e20, 0x6b2d, 0x4f51, { 0x9c, 0x11, 0x3a, 0x7e, 0x5b, 0x90, 0x12, 0x44 } };

/* Local audio, same trust as the 0666 shared-memory ring: SYSTEM, Admins,
   and any logged-in user. The WDK macro is the known-good form of that DACL;
   a hand-written SDDL that IoCreateDeviceSecure rejects leaves the app with
   nothing to open. */

typedef struct HushQueue {
    KSPIN_LOCK Lock;
    float Samples[HUSH_QUEUE_SAMPLES];
    ULONG Read;
    ULONG Count;
} HushQueue;

static HushQueue g_Capture;
static HushQueue g_Render;
static PDEVICE_OBJECT g_ControlDevice = NULL;
static BOOLEAN g_SymlinkReady = FALSE;
static UNICODE_STRING g_Symlink;
static PDRIVER_DISPATCH g_PreviousCreate = NULL;
static PDRIVER_DISPATCH g_PreviousClose = NULL;
static PDRIVER_DISPATCH g_PreviousCleanup = NULL;
static PDRIVER_DISPATCH g_PreviousControl = NULL;

#pragma code_seg()
static void HushQueueInit(_Out_ HushQueue* Queue)
{
    KeInitializeSpinLock(&Queue->Lock);
    RtlZeroMemory(Queue->Samples, sizeof(Queue->Samples));
    Queue->Read = 0;
    Queue->Count = 0;
}

#pragma code_seg()
static void HushQueuePush(_Inout_ HushQueue* Queue, _In_reads_(Count) const float* Src, _In_ ULONG Count)
{
    KIRQL irql;
    ULONG index;
    KeAcquireSpinLock(&Queue->Lock, &irql);
    for (index = 0; index < Count; index++)
    {
        if (Queue->Count == HUSH_QUEUE_SAMPLES)
        {
            Queue->Read = (Queue->Read + 1) % HUSH_QUEUE_SAMPLES;
            Queue->Count--;
        }
        Queue->Samples[(Queue->Read + Queue->Count) % HUSH_QUEUE_SAMPLES] = Src[index];
        Queue->Count++;
    }
    KeReleaseSpinLock(&Queue->Lock, irql);
}

#pragma code_seg()
static BOOLEAN HushQueuePop(_Inout_ HushQueue* Queue, _Out_ float* Sample)
{
    KIRQL irql;
    BOOLEAN got = FALSE;
    KeAcquireSpinLock(&Queue->Lock, &irql);
    if (Queue->Count > 0)
    {
        *Sample = Queue->Samples[Queue->Read];
        Queue->Read = (Queue->Read + 1) % HUSH_QUEUE_SAMPLES;
        Queue->Count--;
        got = TRUE;
    }
    else
    {
        *Sample = 0.0f;
    }
    KeReleaseSpinLock(&Queue->Lock, irql);
    return got;
}

#pragma code_seg()
BOOLEAN HushCapturePop(_Out_ float* Sample)
{
    return HushQueuePop(&g_Capture, Sample);
}

#pragma code_seg()
void HushRenderPush(_In_reads_(Count) const float* Samples, _In_ ULONG Count)
{
    if (Count == 0 || Samples == NULL)
    {
        return;
    }
    HushQueuePush(&g_Render, Samples, Count);
}

#pragma code_seg()
static NTSTATUS HushComplete(_In_ PIRP Irp, _In_ NTSTATUS Status, _In_ ULONG_PTR Information)
{
    Irp->IoStatus.Status = Status;
    Irp->IoStatus.Information = Information;
    IoCompleteRequest(Irp, IO_NO_INCREMENT);
    return Status;
}

#pragma code_seg()
static NTSTATUS HushCreateClose(_In_ PDEVICE_OBJECT DeviceObject, _Inout_ PIRP Irp)
{
    if (DeviceObject != g_ControlDevice)
    {
        if (IoGetCurrentIrpStackLocation(Irp)->MajorFunction == IRP_MJ_CREATE && g_PreviousCreate != NULL)
        {
            return g_PreviousCreate(DeviceObject, Irp);
        }
        if (IoGetCurrentIrpStackLocation(Irp)->MajorFunction == IRP_MJ_CLOSE && g_PreviousClose != NULL)
        {
            return g_PreviousClose(DeviceObject, Irp);
        }
        if (g_PreviousCleanup != NULL)
        {
            return g_PreviousCleanup(DeviceObject, Irp);
        }
    }
    return HushComplete(Irp, STATUS_SUCCESS, 0);
}

#pragma code_seg()
static NTSTATUS HushDeviceControl(_In_ PDEVICE_OBJECT DeviceObject, _Inout_ PIRP Irp)
{
    PIO_STACK_LOCATION stack;
    ULONG code;
    ULONG inLen;
    ULONG outLen;
    HushIoctlAudio* audio;
    ULONG count;
    ULONG index;
    float sample;

    if (DeviceObject != g_ControlDevice)
    {
        if (g_PreviousControl != NULL)
        {
            return g_PreviousControl(DeviceObject, Irp);
        }
        return HushComplete(Irp, STATUS_INVALID_DEVICE_REQUEST, 0);
    }

    stack = IoGetCurrentIrpStackLocation(Irp);
    code = stack->Parameters.DeviceIoControl.IoControlCode;
    inLen = stack->Parameters.DeviceIoControl.InputBufferLength;
    outLen = stack->Parameters.DeviceIoControl.OutputBufferLength;
    audio = (HushIoctlAudio*)Irp->AssociatedIrp.SystemBuffer;

    if (code == HUSH_IOCTL_WRITE_CAPTURE)
    {
        if (audio == NULL || inLen < sizeof(HushIoctlAudio))
        {
            return HushComplete(Irp, STATUS_BUFFER_TOO_SMALL, 0);
        }
        count = audio->frames;
        if (count > HUSH_BRIDGE_SAMPLES)
        {
            count = HUSH_BRIDGE_SAMPLES;
        }
        HushQueuePush(&g_Capture, audio->samples, count);
        return HushComplete(Irp, STATUS_SUCCESS, 0);
    }

    if (code == HUSH_IOCTL_READ_RENDER)
    {
        if (audio == NULL || outLen < sizeof(HushIoctlAudio))
        {
            return HushComplete(Irp, STATUS_BUFFER_TOO_SMALL, 0);
        }
        RtlZeroMemory(audio, sizeof(*audio));
        audio->sample_rate = 48000;
        audio->channels = 1;
        count = 0;
        for (index = 0; index < HUSH_BRIDGE_SAMPLES; index++)
        {
            if (!HushQueuePop(&g_Render, &sample))
            {
                break;
            }
            audio->samples[index] = sample;
            count++;
        }
        audio->frames = count;
        return HushComplete(Irp, STATUS_SUCCESS, sizeof(HushIoctlAudio));
    }

    return HushComplete(Irp, STATUS_INVALID_DEVICE_REQUEST, 0);
}

#pragma code_seg("PAGE")
NTSTATUS HushInstallBridge(_In_ PDRIVER_OBJECT DriverObject)
{
    UNICODE_STRING deviceName;
    NTSTATUS status;

    PAGED_CODE();

    HushQueueInit(&g_Capture);
    HushQueueInit(&g_Render);

    RtlInitUnicodeString(&deviceName, L"\\Device\\HushVad");
    RtlInitUnicodeString(&g_Symlink, L"\\DosDevices\\HushVad");
    /* This WDK exports the SDDL as a UNICODE_STRING, not a wide literal. */
    status = IoCreateDeviceSecure(
        DriverObject,
        0,
        &deviceName,
        FILE_DEVICE_UNKNOWN,
        FILE_DEVICE_SECURE_OPEN,
        FALSE,
        &SDDL_DEVOBJ_SYS_ALL_ADM_RWX_WORLD_RWX_RES_RWX,
        &HUSH_CONTROL_CLASS,
        &g_ControlDevice);
    if (!NT_SUCCESS(status))
    {
        return status;
    }

    g_ControlDevice->Flags |= DO_BUFFERED_IO;
    g_ControlDevice->Flags &= ~DO_DEVICE_INITIALIZING;

    status = IoCreateSymbolicLink(&g_Symlink, &deviceName);
    if (!NT_SUCCESS(status))
    {
        IoDeleteDevice(g_ControlDevice);
        g_ControlDevice = NULL;
        return status;
    }
    g_SymlinkReady = TRUE;

    g_PreviousCreate = DriverObject->MajorFunction[IRP_MJ_CREATE];
    g_PreviousClose = DriverObject->MajorFunction[IRP_MJ_CLOSE];
    g_PreviousCleanup = DriverObject->MajorFunction[IRP_MJ_CLEANUP];
    g_PreviousControl = DriverObject->MajorFunction[IRP_MJ_DEVICE_CONTROL];
    DriverObject->MajorFunction[IRP_MJ_CREATE] = HushCreateClose;
    DriverObject->MajorFunction[IRP_MJ_CLOSE] = HushCreateClose;
    DriverObject->MajorFunction[IRP_MJ_CLEANUP] = HushCreateClose;
    DriverObject->MajorFunction[IRP_MJ_DEVICE_CONTROL] = HushDeviceControl;
    return STATUS_SUCCESS;
}

#pragma code_seg("PAGE")
void HushRemoveBridge(void)
{
    PAGED_CODE();
    if (g_SymlinkReady)
    {
        IoDeleteSymbolicLink(&g_Symlink);
        g_SymlinkReady = FALSE;
    }
    if (g_ControlDevice != NULL)
    {
        IoDeleteDevice(g_ControlDevice);
        g_ControlDevice = NULL;
    }
}
