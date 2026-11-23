# Evaluation data

The repository does not contain DNS Challenge or VoiceBank-DEMAND. Both are large, and the speech corpora have their own terms on top of the code licenses. Generate the tiny synthetic set with:

```bash
python3 bench/make_examples.py
```

That writes `bench/examples/*.wav`. The harness scores those files. Numbers on synthetic vowels are real measurements of the pipeline. They are not a substitute for a speech corpus.

## DNS Challenge

Code and the DNSMOS P.835 scorer are MIT:

- https://github.com/microsoft/DNS-Challenge
- The harness downloads `DNSMOS/DNSMOS/sig_bak_ovr.onnx` into `bench/.cache/` on first use and does not commit it.

The blind test audio is not in that git repository. The official blobs are multi-gigabyte. This run did not download a slice: there is no small official subset that can be fetched without the full archive. VoiceBank-DEMAND is the real-speech result in the README. Do not commit a DNS archive if you download one later.

## VoiceBank-DEMAND

The Valentini VoiceBank-DEMAND set is CC BY 4.0 as distributed by the University of Edinburgh:

- https://datashare.ed.ac.uk/handle/10283/2791
- `clean_testset_wav.zip` and `noisy_testset_wav.zip`

The benchmark downloaded those two zips into `bench/.cache/voicebank/` and did not commit them. `bench/voicebank.py` scores all 824 test files. VCTK and DEMAND still have their own terms if you train on the training split. Do not commit the WAVs.

## PESQ

PESQ is an ITU-T recommendation. The reference implementation is not MIT, Apache, BSD, or MS-PL. Do not add the `pesq` package. Use DNSMOS P.835, STOI, and SI-SDR, which the harness already computes. A lab that has its own ITU license can run PESQ offline on the WAVs the CLI writes.
