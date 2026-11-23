# Other-voice removal

This is not shipped. The experimental setting in the app refuses to start, on purpose.

## What was checked

The job is target-speaker extraction: keep one enrolled voice, remove other voices and noise, in a 10 ms frame at 48 kHz, under a permissive license.

| Candidate | Code license | Weights | Why it is not in the binary |
| --- | --- | --- | --- |
| WeSpeaker ResNet221 / ResNet34 | Apache-2.0 | Apache-2.0 checkpoints | An embedding, not a separator. Useful as a frozen conditioner. |
| SpeechBrain SepFormer, Conv-TasNet | Apache-2.0 | Checkpoints trained on WSJ0-mix and WHAMR! | WSJ0 is LDC, not redistributable. The published checkpoints are not a commercial-clean weight file. |
| SpEx, SpEx+, TD-SpeakerBeam | Research code, mixed | Same WSJ0-2mix family | No permissive streaming checkpoint. |
| ClearVoice / MossFormer, DeepFilterNet target forks | Varies, several non-commercial | Non-commercial or unclear | Out of policy even if the quality is good. |
| NVIDIA NeMo target-speaker models | Apache-2.0 code | NGC terms on the published checkpoints | The checkpoint license is not Apache. |

No streaming model with MIT, Apache-2.0, BSD, or MS-PL weights was found that removes other voices without also shipping a non-commercial checkpoint. DPDFNet and DeepFilterNet3 remove noise. They do not take an enrollment embedding, so they do not do this job.

## Training plan

Train a new causal model. Do not fine-tune a non-commercial checkpoint.

**Conditioner.** Freeze a WeSpeaker ResNet34 embedding (Apache-2.0 code and checkpoint). Compute it on a few seconds of enrollment, once per call, not per frame. Project the 256-d vector with a small affine layer and apply it as FiLM scale and shift on the masker channels.

**Masker.** A causal 48 kHz network the size of `dpdfnet2_48khz_hr` (about 10 MB of fp32 weights, a 20 ms window, a 10 ms hop). Random initialization. Same STFT as the shipping DPDFNet path so the runtime can host it later: Vorbis window, 960-point FFT, hop 480. The network predicts a real mask per bin. It does not see the future.

**Loss.** Signal-approximation on the target waveform, plus a small term that penalizes energy on the interfering speaker. No PESQ in the loss. PESQ is not a permissive dependency.

**Data recipe.** Every source below is CC BY 4.0 or Apache-2.0. Do not add WSJ, LibriMix's WSJ half, AudioSet clips whose files are not cleared, or any non-commercial corpus.

| Role | Source | License |
| --- | --- | --- |
| Target and interferer speech | LibriSpeech `train-clean-360` | CC BY 4.0 |
| Noise | MUSAN noise subset | CC BY 4.0 |
| Enrollment | A different utterance from the target speaker in LibriSpeech | CC BY 4.0 |
| Optional noise | VoiceBank-DEMAND noises already used for the regression test | CC BY 4.0 |

Mix on the fly: one target, one interferer at −5 to 5 dB SIR, MUSAN noise at 0 to 15 dB SNR, 4 second clips, 48 kHz. Enrollment is a clean 3 second crop from the same speaker, never the clip being enhanced. `scripts/target_speaker/recipe.py` writes that manifest and refuses a path whose license is not on the list.

**GPU estimate, not a measurement.** A 3 million parameter causal model, 100 hours of mixtures, batch 16 of 4 second clips, is on the order of one day on one A100 40 GB. 500 hours and 30 epochs is a few days on that same GPU. CPU training is not useful. No GPU was available here, and no weights were produced.

**When it would ship.** Only after a VoiceBank-style test where the interferer is a second speaker, the weights are trained from this recipe, and the license file lists Apache-2.0 or MIT for both code and weights. Until then the setting stays an error.
