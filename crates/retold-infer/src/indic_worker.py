"""Pinned IndicF5 worker. stdout is exclusively the v1 JSON-lines protocol."""
import contextlib
import json
import os
import sys
from pathlib import Path

os.environ["HF_HUB_OFFLINE"] = "1"
os.environ["TRANSFORMERS_OFFLINE"] = "1"


def load_model(device):
    # Build the published architecture directly instead of running the Hub's
    # remote code, which downloads the vocoder and vocabulary at load time.
    from safetensors.torch import load_file
    from vocos import Vocos
    from f5_tts.infer.utils_infer import load_model as build
    from f5_tts.model import DiT
    state = load_file(root / "model.safetensors")
    def part(prefix):
        return {k[len(prefix):]: v for k, v in state.items() if k.startswith(prefix)}
    model = build(DiT, dict(dim=1024, depth=22, heads=16, ff_mult=2, text_dim=512, conv_layers=4), mel_spec_type="vocos", vocab_file=str(root / "checkpoints/vocab.txt"), device="cpu")
    model.load_state_dict(part("ema_model._orig_mod."))
    vocoder = Vocos.from_hparams(str(root / "vocos/config.yaml"))
    vocoder.load_state_dict(part("vocoder._orig_mod."))
    return model.to(device).eval(), vocoder.to(device).eval()


model = None
root = Path(sys.argv[1]).resolve()
for line in sys.stdin:
    request = json.loads(line)
    response = {"version": 1, "id": request.get("id")}
    try:
        if request.get("version") != 1:
            raise ValueError("unsupported protocol version")
        with contextlib.redirect_stdout(sys.stderr):
            if request["operation"] == "health":
                import importlib.metadata
                result = {"ready": True, "package": importlib.metadata.version("f5-tts"), "loaded": model is not None}
            elif request["operation"] == "synthesize":
                import io
                import numpy as np
                import soundfile as sf
                import torch
                from pydub import AudioSegment, silence
                from f5_tts.infer.utils_infer import infer_process, preprocess_ref_audio_text
                device = "mps" if torch.backends.mps.is_available() else "cpu"
                if model is None:
                    model = load_model(device)
                params = request["params"]
                speed = params.get("speed", 1.0)
                if not 0.75 <= speed <= 1.25:
                    raise ValueError("speed outside 0.75–1.25")
                if not params.get("sample") or not params.get("sample_text", "").strip():
                    raise ValueError("IndicF5 needs a reference sample and its transcript")
                torch.manual_seed(params.get("seed", 42))
                # The caller already cut the reference to ≤15 s and transcribed
                # that exact audio; clipping again would desynchronise them.
                reference, reference_text = preprocess_ref_audio_text(params["sample"], params["sample_text"], clip_short=False, show_info=lambda *_: None)
                wav, sr, _ = infer_process(reference, reference_text, params["text"], model[0], model[1], mel_spec_type="vocos", speed=speed, device=device, show_info=lambda *_: None, progress=None)
                os.unlink(reference)
                # Match the published inference: drop long silences, then
                # normalise to -20 dBFS.
                buffer = io.BytesIO()
                sf.write(buffer, wav, samplerate=sr, format="WAV")
                buffer.seek(0)
                segment = AudioSegment.from_file(buffer, format="wav")
                segment = sum(silence.split_on_silence(segment, min_silence_len=1000, silence_thresh=-50, keep_silence=500, seek_step=10), AudioSegment.silent(duration=0))
                segment = segment.apply_gain(-20.0 - segment.dBFS)
                values = np.array(segment.get_array_of_samples()).astype(np.float32) / 32768.0
                destination = Path(params["output"])
                temporary = destination.with_suffix(".partial.wav")
                sf.write(str(temporary), values, sr, subtype="PCM_16")
                os.replace(temporary, destination)
                result = {"path": str(destination), "sample_rate": sr}
            else:
                raise ValueError("unknown worker operation")
        response["result"] = result
    except Exception as error:
        response["error"] = str(error)
    print(json.dumps(response), flush=True)
