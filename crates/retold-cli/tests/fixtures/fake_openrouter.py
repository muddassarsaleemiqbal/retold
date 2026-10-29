"""A local stand-in for the OpenRouter API, so a real CLI run can reach `ready` offline.

It mirrors the engine's in-process TextFake and MediaFake: structured text answers are chosen
by the requested schema, images are flat PNGs of the requested aspect, speech is a quiet tone, and
transcripts echo the text that was last spoken in that language. Every price is zero.

Usage: python3 fake_openrouter.py <fixture-dir> <port-file>
The chosen port is written to <port-file> once the server is listening.
"""
import base64
import json
import math
import re
import struct
import sys
import threading
import zlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse

FIXTURES = None
SPEECH_SECONDS = 100
SAMPLE_RATE = 16_000
SHORT_SIDE = 720
TONE_PERIOD = 80  # 200 Hz at 16 kHz
TONE_AMPLITUDE = 3_000
spoken = {}
spoken_lock = threading.Lock()


def fixture(name):
    with open(f"{FIXTURES}/{name}", encoding="utf-8") as handle:
        return json.load(handle)


def after(prompt, marker):
    tail = prompt.split(marker, 1)[1].lstrip()
    value, _ = json.JSONDecoder().raw_decode(tail)
    return value


def palette(name):
    return "#%06x" % (len(name) * 0x01_0203 % 0x00FF_FFFF)


def character(name):
    return {
        "name": name, "appearance": "Oval face and expressive brows",
        "clothes": "Plain slate coat", "palette": [palette(name), "#EEE6D7"],
        "distinguishing_detail": "A narrow cream scarf", "age": "late thirties",
        "build": "7.5 heads tall, lean, upright",
        "face": "Oval head, dot eyes, thin brows, level mouth",
        "hair": "Short dark side-parted hair", "voice": "male, late thirties, warm",
        "colors": [{"part": "skin", "hex": "#E8C9A8"}, {"part": "coat", "hex": palette(name)},
                   {"part": "scarf", "hex": "#EEE6D7"}],
    }


def shots(prompt):
    parallax = "Parallax rules: enabled=true" in prompt
    return {"shots": [
        {"unit": unit["id"], "shot": "wide" if index % 2 == 0 else "medium",
         "location": "Wisborg", "action": "An original character waits beside a window",
         "mood": "grave", "characters": [], "focus": [0.5, 0.4],
         "motion": "parallax" if index < 2 and parallax else None,
         "parallax_move": "orbit" if index < 2 and parallax else None, "hero": index == 0}
        for index, unit in enumerate(after(prompt, "Units: "))]}


def transcreated(prompt):
    master = after(prompt, "Master: ")["units"]
    urdu = "Urdu" in prompt
    count = len(master)
    base, remainder = divmod(280, count)
    units = []
    for index, unit in enumerate(master):
        size = base + (1 if index < remainder else 0)
        stem = "لفظ" if urdu else "शब्द"
        words = [f"{stem}{index * size + word}" for word in range(size)]
        stop, ask = ("۔", "؟") if urdu else ("।", "?")
        if index % 3 == 0:
            words[size // 2 - 1] += stop
        words[-1] += ask if index + 1 == count else stop
        units.append({"id": unit["id"], "section": unit["section"], "text": " ".join(words)})
    return {"units": units}


def figures(prompt):
    match = re.search(r"exactly (\d+) ", prompt)
    return int(match.group(1)) if match else 1


def answer(prompt, schema):
    fields = schema.get("properties", {})
    if "Respell the following" in prompt:
        return {"text": prompt.split("Expected: ")[1].split(". Heard:")[0]}
    by_field = [
        ("beats", lambda: fixture("plan.json")),
        ("candidates", lambda: fixture("hooks.json")),
        ("winner", lambda: {"winner": 0, "reason": "fixture selection",
                            "scores": [9.0] * (2 if "Blind drafts" in prompt else 5)}),
        ("sections", lambda: fixture("script.json")),
        ("faces", lambda: {"faces": []}),
        ("issues", lambda: {"issues": []}),
        ("shots", lambda: shots(prompt)),
        ("characters", lambda: {"characters": [
            character(name) for name in after(prompt, "names exactly once: ")]}),
        ("same_character", lambda: {"figures": figures(prompt), "same_character": True,
                                    "text_present": False, "anatomy_errors": [],
                                    "problems": []}),
        ("style_score", lambda: {"style_score": 9.0, "character_score": 9.0,
                                 "text_present": False, "anatomy_errors": [], "problems": [],
                                 "focus": [0.5, 0.4]}),
        ("units", lambda: transcreated(prompt)),
        ("words", lambda: {"words": [f"शब्द{index}" for index, _ in
                                     enumerate(after(prompt, "Input: "))]}),
        ("phrases", lambda: {"phrases": [{"id": phrase["id"],
                                          "text": "The visitor waits at the window."}
                                         for phrase in after(prompt, "Phrases: ")]}),
        ("ranking", lambda: {"ranking": [
            {"candidate": candidate, "score": 10.0 - candidate, "usable": True,
             "reason": "Fixture candidate has visible subject and legible composition"}
            for candidate in (1, 2, 3)]}),
        ("ranking", lambda: {"ranking": [
            {"candidate": candidate, "score": 9.0 - candidate, "usable": True,
             "reason": "Clear subject and legible title"} for candidate in (1, 2, 3)]}),
        ("posts", lambda: {"posts": [
            {"platform": platform, "title": "Nosferatu: the open window",
             "description": "Spoilers for Nosferatu. An animated recap with AI-generated art.",
             "hashtags": ["#Nosferatu", "#Story", "#Recap", "#Animation", "#Film"]}
            for platform in ["youtube", "tiktok", "instagram", "facebook"]]}),
    ]
    for field, build in by_field:
        if field in fields:
            return build()
    raise ValueError(f"unhandled fixture schema: {json.dumps(schema)[:400]}")


def png(aspect):
    across, down = (int(part) for part in aspect.split(":"))
    scale = SHORT_SIDE / min(across, down)
    width, height = round(across * scale), round(down * scale)
    row = b"\x00" + bytes((230, 220, 205)) * width
    pixels = zlib.compress(row * height)

    def chunk(kind, body):
        return (struct.pack(">I", len(body)) + kind + body
                + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF))
    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", pixels)
            + chunk(b"IEND", b""))


def wav(seconds):
    # A steady quiet tone rather than silence, so loudness can be measured and corrected.
    period = [round(TONE_AMPLITUDE * math.sin(2 * math.pi * index / TONE_PERIOD))
              for index in range(TONE_PERIOD)]
    cycle = struct.pack(f"<{TONE_PERIOD}h", *period)
    data = cycle * (SAMPLE_RATE * seconds // TONE_PERIOD)
    fmt = struct.pack("<HHIIHH", 1, 1, SAMPLE_RATE, SAMPLE_RATE * 2, 2, 16)
    body = b"WAVE" + b"fmt " + struct.pack("<I", len(fmt)) + fmt
    body += b"data" + struct.pack("<I", len(data)) + data
    return b"RIFF" + struct.pack("<I", len(body)) + body


def transcript(language):
    with spoken_lock:
        text = spoken.get(language) or next(iter(spoken.values()), "")
    tokens = text.split()
    step = SPEECH_SECONDS / max(len(tokens), 1)
    words = [{"word": token, "start": index * step, "end": (index + 1) * step - 0.002}
             for index, token in enumerate(tokens)]
    return {"text": text, "duration": float(SPEECH_SECONDS), "words": words,
            "usage": {"cost": 0}}


MODELS = {"data": [
    {"id": model, "pricing": {"prompt": 0, "completion": 0},
     "architecture": {"output_modalities": modalities}}
    for model, modalities in [("fixture/text", ["text"]), ("fixture/vision", ["text"]),
                              ("fixture/tts", ["speech"]), ("fixture/whisper", ["text"])]]}
ENDPOINTS = {"data": {"endpoints": [
    {"pricing": {"prompt": 0, "completion": 0}, "max_completion_tokens": 24_000}]}}
IMAGE_ENDPOINTS = {"endpoints": [{"provider_tag": "fixture", "pricing": [
    {"billable": "output_image", "unit": "image", "cost_usd": 0}]}]}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def reply(self, payload, kind="application/json"):
        body = payload if isinstance(payload, bytes) else json.dumps(payload).encode()
        self.send_response(200)
        self.send_header("Content-Type", kind)
        # Costs are confirmed through /generation, as OpenRouter does; every price is zero.
        self.send_header("x-generation-id", "fixture-generation")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def fail(self, message):
        body = json.dumps({"error": {"message": message}}).encode()
        self.send_response(400)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
        sys.stderr.write(message + "\n")

    def do_GET(self):
        path = urlparse(self.path).path
        if path == "/models":
            return self.reply(MODELS)
        if path.startswith("/images/models/"):
            return self.reply(IMAGE_ENDPOINTS)
        if path.startswith("/models/"):
            return self.reply(ENDPOINTS)
        if path == "/generation":
            return self.reply({"data": {"total_cost": 0}})
        if path == "/key":
            return self.reply({"data": {"usage": 0, "limit": None}})
        return self.fail(f"unhandled GET {self.path}")

    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        request = json.loads(self.rfile.read(length) or b"{}")
        try:
            return self.route(urlparse(self.path).path, request)
        except Exception as error:  # report every fixture gap to the run that hit it
            return self.fail(f"{self.path}: {error}")

    def route(self, path, request):
        if path == "/chat/completions":
            prompt = "".join(part.get("text", "") for message in request["messages"]
                             if message["role"] == "user" for part in message["content"])
            schema = request["response_format"]["json_schema"]["schema"]
            value = answer(prompt, schema)
            return self.reply({"choices": [{"message": {"content": json.dumps(value)}}],
                               "usage": {"prompt_tokens": 1, "completion_tokens": 1,
                                         "cost": 0}})
        if path == "/images":
            image = base64.b64encode(png(request["aspect_ratio"])).decode()
            return self.reply({"data": [{"b64_json": image}], "usage": {"cost": 0}})
        if path == "/audio/speech":
            text = re.sub(r"\[[^\]]*\]", "", request["input"])
            with spoken_lock:
                spoken[request.get("language", "")] = text
                spoken["latest"] = text
            return self.reply(wav(SPEECH_SECONDS), "audio/wav")
        if path == "/audio/transcriptions":
            return self.reply(transcript(request.get("language", "")))
        return self.fail(f"unhandled POST {path}")


if __name__ == "__main__":
    FIXTURES = sys.argv[1]
    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    with open(sys.argv[2], "w", encoding="utf-8") as handle:
        handle.write(str(server.server_address[1]))
    server.serve_forever()
