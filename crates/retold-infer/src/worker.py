"""Pinned Chatterbox worker. stdout is exclusively the v1 JSON-lines protocol."""
import contextlib
import json
import os
import sys
from pathlib import Path

os.environ["HF_HUB_OFFLINE"] = "1"
os.environ["TRANSFORMERS_OFFLINE"] = "1"
def load_model(device):
    # The pinned upstream loader omits map_location on CUDA-saved checkpoints.
    # Load on CPU first, then move to MPS/CPU, without modifying package globals.
    import torch
    from safetensors.torch import load_file
    from chatterbox.mtl_tts import (
        ChatterboxMultilingualTTS, VoiceEncoder, T3, T3Config,
        S3Gen, MTLTokenizer, Conditionals,
    )
    encoder = VoiceEncoder()
    encoder.load_state_dict(torch.load(root / "ve.pt", map_location="cpu", weights_only=True))
    encoder.to(device).eval()
    if pack:
        class HindiPackConfig(T3Config):
            # The official Hindi V3 inference removes the V2 attention-based
            # early-stop analyzer. In pinned 0.1.6, this flag is used only to
            # enable that analyzer; keep the actual multilingual vocabulary.
            @property
            def is_multilingual(self):
                return False
        t3_config = HindiPackConfig.multilingual()
    else:
        t3_config = T3Config.multilingual()
    t3 = T3(t3_config)
    state = load_file(pack / "t3_hi.safetensors" if pack else root / "t3_mtl23ls_v2.safetensors")
    if "model" in state:
        state = state["model"][0]
    t3.load_state_dict(state)
    t3.to(device).eval()
    generator = S3Gen()
    decoder_state = torch.load(pack / "s3gen_v3.pt" if pack else root / "s3gen.pt", map_location="cpu", weights_only=True)
    if pack:
        # V3 no longer persists these deterministic tokenizer buffers. Keep the
        # pinned package's initialized Hann window/mel filterbank, while still
        # requiring every trained weight and rejecting unexpected keys.
        missing, unexpected = generator.load_state_dict(decoder_state, strict=False)
        if set(missing) - {"tokenizer._mel_filters", "tokenizer.window"} or unexpected:
            raise ValueError(f"incompatible Hindi decoder: missing={missing}, unexpected={unexpected}")
    else:
        generator.load_state_dict(decoder_state)
    generator.to(device).eval()
    if pack:
        class HindiPackTokenizer(MTLTokenizer):
            def preprocess_text(self, raw_text, language_id=None, lowercase=True, nfkd_normalize=True):
                # Match the Hindi checkpoint's published tokenizer, which uses
                # the original graphemes rather than the later V2 NFKD pass.
                return raw_text
        tokenizer = HindiPackTokenizer(str(root / "grapheme_mtl_merged_expanded_v1.json"))
    else:
        tokenizer = MTLTokenizer(str(root / "grapheme_mtl_merged_expanded_v1.json"))
    conditions = Conditionals.load(root / "conds.pt", map_location="cpu").to(device)
    return ChatterboxMultilingualTTS(t3, generator, encoder, tokenizer, device, conditions)

model = None
default_conditions = None
root = Path(__file__).resolve().parent
pack = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else None
for line in sys.stdin:
    request = json.loads(line)
    response = {"version": 1, "id": request.get("id")}
    try:
        if request.get("version") != 1:
            raise ValueError("unsupported protocol version")
        with contextlib.redirect_stdout(sys.stderr):
            if request["operation"] == "health":
                import importlib.metadata
                from perth.perth_net.perth_net_implicit.perth_watermarker import PerthImplicitWatermarker
                result = {"ready": True, "package": importlib.metadata.version("chatterbox-tts"), "loaded": model is not None}
            elif request["operation"] == "embedding":
                import librosa
                import torch
                if model is None:
                    device = "mps" if torch.backends.mps.is_available() else "cpu"
                    model = load_model(device)
                    default_conditions = model.conds
                wav, _ = librosa.load(request["params"]["path"], sr=16000)
                embedding = model.ve.embeds_from_wavs([wav], sample_rate=16000).mean(axis=0)
                result = {"embedding": embedding.tolist()}
            elif request["operation"] == "synthesize":
                import torch
                import torchaudio
                if model is None:
                    device = "mps" if torch.backends.mps.is_available() else "cpu"
                    model = load_model(device)
                    default_conditions = model.conds
                params = request["params"]
                torch.manual_seed(params.get("seed", 42))
                # Reset default conditioning so a previous cloned request never leaks its voice.
                model.conds = default_conditions
                wav = model.generate(params["text"], language_id=params["language"], audio_prompt_path=params.get("sample"), exaggeration=params.get("exaggeration", 0.5), cfg_weight=params.get("cfg_weight", 0.5), temperature=params.get("temperature", 0.8), top_p=0.95 if pack else 1.0, repetition_penalty=1.2 if pack else 2.0)
                if pack and wav.shape[-1] > model.sr // 25:
                    # The dedicated Hindi demo drops the degraded final token
                    # (~40 ms), before writing its audio.
                    wav = wav[..., :-(model.sr // 25)]
                speed = params.get("speed", 1.0)
                if not 0.75 <= speed <= 1.25:
                    raise ValueError("speed outside 0.75–1.25")
                if speed != 1.0:
                    import numpy as np
                    import librosa
                    values = librosa.effects.time_stretch(wav.cpu().numpy().reshape(-1), rate=speed)
                    wav = torch.from_numpy(values).unsqueeze(0)
                destination = Path(params["output"])
                temporary = destination.with_suffix(".partial.wav")
                torchaudio.save(str(temporary), wav.cpu(), model.sr)
                os.replace(temporary, destination)
                result = {"path": str(destination), "sample_rate": model.sr}
            else:
                raise ValueError("unknown worker operation")
        response["result"] = result
    except Exception as error:
        response["error"] = str(error)
    print(json.dumps(response), flush=True)
