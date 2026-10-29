"""Offline acoustic IPA phoneme timing. Pinned weights; no Hub code or runtime downloads."""
import contextlib
import json
import sys
from pathlib import Path


class AlignmentRejected(ValueError):
    """Audio cannot support reliable timing; safe still fallback is appropriate."""


def decode(logp, labels, begin, windows):
    """CTC phone runs from the actual pronunciation, with 20 ms acoustic timing."""
    import numpy as np
    ids = logp.argmax(axis=-1)
    result = []
    at = 0
    while at < len(ids):
        end = at + 1
        while end < len(ids) and ids[end] == ids[at]:
            end += 1
        token = int(ids[at])
        if token > 3:
            start_time = begin + (at * 320 + 40) / 16000
            end_time = begin + ((end - 1) * 320 + 360) / 16000
            center = (start_time + end_time) / 2
            # Keep only the supplied speech, including small word-ASR boundary error.
            choices = [(i, a, b) for i, (a, b) in enumerate(windows) if a - .12 <= center <= b + .12]
            if choices:
                word, _, _ = min(choices, key=lambda w: abs(center - (w[1] + w[2]) / 2))
                result.append(dict(ipa=labels[token], start=start_time, end=end_time,
                    confidence=float(np.exp(logp[at:end, token].mean())), word=word))
        at = end
    if not result:
        raise AlignmentRejected("no acoustic phones in the supplied speech")
    for a, b in zip(result, result[1:]):
        if a["word"] == b["word"]:
            boundary = (a["end"] + b["start"]) / 2
            a["end"] = boundary
            b["start"] = boundary
    for phone in result:
        del phone["word"]
    if float(np.median([p["confidence"] for p in result])) < .2:
        raise AlignmentRejected("acoustic phoneme confidence is too low")
    return result


def align(model, params, root):
    import numpy as np
    import soundfile as sf
    import torch
    from scipy.signal import resample_poly
    from math import gcd
    audio, rate = sf.read(params["audio"], dtype="float32", always_2d=True)
    audio = audio.mean(axis=1)
    words = params["words"]
    if not words or any(not 0 <= w["start"] < w["end"] <= len(audio) / rate + .001 for w in words):
        raise AlignmentRejected("invalid word/audio bounds")
    if any(a["end"] > b["start"] + .001 for a, b in zip(words, words[1:])):
        raise AlignmentRejected("overlapping word boundaries")
    begin = max(0, words[0]["start"] - .12)
    end = min(len(audio) / rate, words[-1]["end"] + .12)
    if end - begin > 45:
        raise AlignmentRejected("align at most 45 seconds per request")
    audio = audio[int(begin * rate):int(end * rate)]
    divisor = gcd(rate, 16000)
    audio = resample_poly(audio, 16000 // divisor, rate // divisor)
    if float(audio.std()) < 1e-4:
        raise AlignmentRejected("audio is silent")
    audio = (audio - audio.mean()) / np.sqrt(audio.var() + 1e-7)
    with torch.inference_mode():
        logits = model(torch.tensor(audio).unsqueeze(0)).logits[0]
        logp = logits.log_softmax(-1).cpu().numpy()
    vocab = json.loads((root / "vocab.json").read_text())
    labels = {value: label for label, value in vocab.items()}
    windows = [(w["start"], w["end"]) for w in words]
    result = decode(logp, labels, begin, windows)
    return dict(phonemes=result, stride_seconds=.02, model_revision=root.name)


def main():
    root = Path(sys.argv[1]).resolve()
    model = None
    for line in sys.stdin:
        request = json.loads(line)
        response = dict(version=1, id=request.get("id"))
        try:
            with contextlib.redirect_stdout(sys.stderr):
                if request.get("version") != 1:
                    raise ValueError("unsupported protocol")
                if request["operation"] == "health":
                    result = dict(ready=True, loaded=model is not None)
                elif request["operation"] == "align":
                    if model is None:
                        import torch
                        from transformers import Wav2Vec2ForCTC
                        torch.set_num_threads(4)
                        model = Wav2Vec2ForCTC.from_pretrained(root, local_files_only=True).eval()
                    result = align(model, request["params"], root)
                else:
                    raise ValueError("unknown operation")
            response["result"] = result
        except Exception as error:
            response["error"] = ("quality: " if isinstance(error, AlignmentRejected) else "") + str(error)
        print(json.dumps(response), flush=True)


if __name__ == "__main__":
    main()
