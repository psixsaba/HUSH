// HUSH Microphone and HUSH Speaker. The plugin is fed by the HUSH userspace pipeline.
//
// The plugin does not run the neural net. `hush run` or the desktop app writes
// mono 48 kHz float frames into the POSIX shared-memory segment, and this
// driver copies them out in the device's native float32 stereo format. Volume
// and mute stay on the default libASPL processing path.

#include <aspl/Driver.hpp>
#include <aspl/IORequestHandler.hpp>
#include <aspl/Plugin.hpp>

#include <CoreFoundation/CoreFoundation.h>

#include "hush_ring.h"

#include <algorithm>
#include <cstring>
#include <memory>

namespace {

constexpr UInt32 kSampleRate = 48000;
constexpr UInt32 kChannels = 2;
constexpr UInt32 kMaxFrames = 4096;

AudioStreamBasicDescription FloatStereo()
{
    AudioStreamBasicDescription format = {};
    format.mSampleRate = kSampleRate;
    format.mFormatID = kAudioFormatLinearPCM;
    format.mFormatFlags =
        kAudioFormatFlagIsFloat | kAudioFormatFlagsNativeEndian | kAudioFormatFlagIsPacked;
    format.mBitsPerChannel = 32;
    format.mChannelsPerFrame = kChannels;
    format.mBytesPerFrame = sizeof(Float32) * kChannels;
    format.mFramesPerPacket = 1;
    format.mBytesPerPacket = format.mBytesPerFrame;
    return format;
}

class SpeakerHandler : public aspl::ControlRequestHandler, public aspl::IORequestHandler
{
public:
    OSStatus OnStartIO() override
    {
        Attach();
        return kAudioHardwareNoError;
    }

    void OnWriteMixedOutput(const std::shared_ptr<aspl::Stream>&,
        Float64,
        Float64,
        const void* bytes,
        UInt32 bytesCount) override
    {
        if (!bytes || bytesCount == 0) {
            return;
        }
        const UInt32 frameBytes = sizeof(Float32) * kChannels;
        if (bytesCount % frameBytes != 0) {
            return;
        }
        if (!ring_) {
            if (++retry_ == 50) {
                retry_ = 0;
                Attach();
            }
            if (!ring_) {
                return;
            }
        }
        const auto* interleaved = static_cast<const Float32*>(bytes);
        UInt32 frames = bytesCount / frameBytes;
        UInt32 done = 0;
        while (done < frames) {
            UInt32 chunk = std::min(kMaxFrames, frames - done);
            for (UInt32 i = 0; i < chunk; ++i) {
                const Float32 left = interleaved[(done + i) * kChannels];
                const Float32 right = interleaved[(done + i) * kChannels + 1];
                mono_[i] = 0.5f * (left + right);
            }
            hush_ring_write(ring_, mono_, chunk);
            done += chunk;
        }
    }

private:
    void Attach()
    {
        if (ring_) {
            return;
        }
        ring_ = hush_ring_attach_named(HUSH_SHM_SPEAKER);
    }

    HushRing* ring_ = nullptr;
    Float32 mono_[kMaxFrames] = {};
    UInt32 retry_ = 0;
};

class MicHandler : public aspl::ControlRequestHandler, public aspl::IORequestHandler
{
public:
    explicit MicHandler(std::shared_ptr<aspl::Device> device)
        : device_(std::move(device))
    {
    }

    OSStatus OnStartIO() override
    {
        Attach();
        if (reader_) {
            hush_ring_reader_reset(reader_);
        }
        PublishLatency();
        return kAudioHardwareNoError;
    }

    void OnStopIO() override
    {
        if (reader_) {
            hush_ring_reader_reset(reader_);
        }
    }

    void OnReadClientInput(const std::shared_ptr<aspl::Client>&,
        const std::shared_ptr<aspl::Stream>&,
        Float64,
        Float64 timestamp,
        void* bytes,
        UInt32 bytesCount) override
    {
        if (!bytes || bytesCount == 0) {
            return;
        }
        const UInt32 frameBytes = sizeof(Float32) * kChannels;
        if (bytesCount % frameBytes != 0) {
            std::memset(bytes, 0, bytesCount);
            return;
        }
        if (!ring_) {
            if (++retry_ == 50) {
                retry_ = 0;
                Attach();
            }
            if (!ring_) {
                std::memset(bytes, 0, bytesCount);
                return;
            }
        }

        auto* interleaved = static_cast<Float32*>(bytes);
        UInt32 frames = bytesCount / frameBytes;
        UInt32 done = 0;
        while (done < frames) {
            UInt32 chunk = std::min(kMaxFrames, frames - done);
            uint32_t missed = hush_ring_read(reader_, timestamp + done, mono_, chunk);
            (void)missed;
            for (UInt32 i = 0; i < chunk; ++i) {
                interleaved[(done + i) * kChannels] = mono_[i];
                interleaved[(done + i) * kChannels + 1] = mono_[i];
            }
            done += chunk;
        }
    }

private:
    void Attach()
    {
        if (ring_) {
            return;
        }
        ring_ = hush_ring_attach();
        if (!ring_) {
            return;
        }
        reader_ = hush_ring_reader_new(ring_);
        if (!reader_) {
            hush_ring_destroy(ring_);
            ring_ = nullptr;
            return;
        }
        PublishLatency();
    }

    void PublishLatency()
    {
        if (!ring_) {
            return;
        }
        auto device = device_.lock();
        if (!device) {
            return;
        }
        // The publisher's model delay, plus the 20 ms jitter the reader
        // keeps so a late callback does not click.
        const UInt32 latency = hush_ring_latency(ring_) + kSampleRate / 50;
        device->SetLatencyAsync(latency);
    }

    std::weak_ptr<aspl::Device> device_;
    HushRing* ring_ = nullptr;
    HushRingReader* reader_ = nullptr;
    Float32 mono_[kMaxFrames] = {};
    UInt32 retry_ = 0;
};

std::shared_ptr<aspl::Driver> CreateDriver()
{
    auto context = std::make_shared<aspl::Context>();

    aspl::DeviceParameters deviceParams;
    deviceParams.Name = "HUSH Microphone";
    deviceParams.Manufacturer = "HUSH";
    deviceParams.DeviceUID = "com.hush.microphone";
    deviceParams.ModelUID = "hush.mic.model";
    deviceParams.SerialNumber = "0.1.0";
    deviceParams.SampleRate = kSampleRate;
    deviceParams.ChannelCount = kChannels;
    // DeepFilterNet3's 30 ms plus the frame quantum and the ring jitter.
    // OnStartIO replaces this with the latency the publisher is actually using.
    deviceParams.Latency = 1440 + 480 + kSampleRate / 50;
    deviceParams.SafetyOffset = 0;
    deviceParams.CanBeDefault = true;
    deviceParams.CanBeDefaultForSystemSounds = false;
    deviceParams.ClockIsStable = true;
    deviceParams.EnableRealtimeTracing = false;

    auto device = std::make_shared<aspl::Device>(context, deviceParams);

    aspl::StreamParameters streamParams;
    streamParams.Direction = aspl::Direction::Input;
    streamParams.StartingChannel = 1;
    streamParams.Format = FloatStereo();
    streamParams.Latency = 0;
    device->AddStreamWithControlsAsync(streamParams);

    auto handler = std::make_shared<MicHandler>(device);
    device->SetControlHandler(handler);
    device->SetIOHandler(handler);

    aspl::DeviceParameters speakerParams;
    speakerParams.Name = "HUSH Speaker";
    speakerParams.Manufacturer = "HUSH";
    speakerParams.DeviceUID = "com.hush.speaker";
    speakerParams.ModelUID = "hush.spk.model";
    speakerParams.SampleRate = kSampleRate;
    speakerParams.ChannelCount = kChannels;
    speakerParams.Latency = kSampleRate / 50;
    speakerParams.CanBeDefault = true;
    speakerParams.CanBeDefaultForSystemSounds = false;
    speakerParams.EnableRealtimeTracing = false;

    auto speaker = std::make_shared<aspl::Device>(context, speakerParams);
    aspl::StreamParameters speakerStream;
    speakerStream.Direction = aspl::Direction::Output;
    speakerStream.StartingChannel = 1;
    speakerStream.Format = FloatStereo();
    speaker->AddStreamWithControlsAsync(speakerStream);
    auto speakerHandler = std::make_shared<SpeakerHandler>();
    speaker->SetControlHandler(speakerHandler);
    speaker->SetIOHandler(speakerHandler);

    auto plugin = std::make_shared<aspl::Plugin>(context);
    plugin->AddDevice(device);
    plugin->AddDevice(speaker);
    return std::make_shared<aspl::Driver>(context, plugin);
}

} // namespace

extern "C" void* HushMicEntryPoint(CFAllocatorRef, CFUUIDRef typeUUID)
{
    if (!CFEqual(typeUUID, kAudioServerPlugInTypeUUID)) {
        return nullptr;
    }
    static std::shared_ptr<aspl::Driver> driver = CreateDriver();
    return driver->GetReference();
}
