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

The blind test sets and the synthesized training noise are distributed separately, often as multi-gigabyte archives linked from the repository README. Read that README for the current download and for the data terms before using the clips in a commercial comparison. Do not commit the archives.

## VoiceBank-DEMAND

The Valentini VoiceBank-DEMAND set (clean VCTK sentences mixed with DEMAND noise) is the usual wideband baseline:

- https://datashare.ed.ac.uk/handle/10283/2791

VCTK and DEMAND each have their own license. Check both before training or before publishing a commercial claim. A typical layout after download is `clean/` and `noisy/` WAVs at 48 kHz. Point a copy of the harness at those paths; do not add the files to git.

## PESQ

PESQ is an ITU-T recommendation. The reference implementation is not MIT, Apache, BSD, or MS-PL. Do not add the `pesq` package. Use DNSMOS P.835, STOI, and SI-SDR, which the harness already computes. A lab that has its own ITU license can run PESQ offline on the WAVs the CLI writes.
