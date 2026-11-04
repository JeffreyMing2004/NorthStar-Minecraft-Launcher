"""Original soundtrack, preview sheet, and encoded video checks for Fluent promo."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import wave

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "target" / "promo-deps"))
import imageio_ffmpeg
import numpy as np
from PIL import Image
import qrcode

CACHE = ROOT / "target" / "promo-fluent"
OUT = ROOT / "docs" / "promo" / "fluent"
DOWNLOAD = "https://github.com/JeffreyMing2004/NorthStar-Minecraft-Launcher/releases"


def music():
    sr, duration, beat = 48000, 60, .5
    audio = np.zeros((sr * duration, 2), dtype=np.float32)
    rng = np.random.default_rng(1126)

    def put(start, signal, gain=1, pan=0):
        at = round(start * sr)
        signal = signal[:len(audio)-at] * gain
        if at < 0 or not len(signal):
            return
        audio[at:at+len(signal), 0] += signal * np.sqrt((1-pan)/2)
        audio[at:at+len(signal), 1] += signal * np.sqrt((1+pan)/2)

    def note(start, length, notes, gain, pan=0, decay=None):
        t = np.arange(round(length*sr), dtype=np.float32)/sr
        sound = np.zeros_like(t)
        for midi in notes:
            freq = 440 * 2 ** ((midi-69)/12)
            sound += np.sin(2*np.pi*freq*t) + .14*np.sin(2*np.pi*freq*2*t)
        sound /= len(notes)
        env = np.minimum(t/.02, 1) * np.minimum((length-t)/.25, 1)
        if decay:
            env *= np.exp(-t*decay)
        put(start, sound*np.maximum(env, 0), gain, pan)

    chords = [(50,54,57,61),(47,50,54,57),(43,47,50,54),(45,49,52,57)]
    for bar in range(30):
        chord, start = chords[bar%4], bar*2
        rise = .5 + .5*min(start/30, 1)
        note(start, 2.8, chord, .09*rise, -.25)
        note(start+.06, 2.75, [n+12 for n in chord], .025*rise, .3)
        if bar >= 3:
            for k in range(8):
                note(start+k*.25, 1, [chord[[0,2,1,3,2,1,3,2][k]]+12], .08*rise,
                     -.65 if k%2 else .65, 5)
        if 5 <= bar < 27:
            for k in range(4):
                note(start+k*beat, .42, [chord[0]-12], .12)
                t = np.arange(int(.2*sr))/sr
                kick = np.sin(2*np.pi*(43*t+5*(1-np.exp(-t*30))))*np.exp(-t*26)
                put(start+k*beat, kick, .17)
                if k%2:
                    t = np.arange(int(.13*sr))/sr
                    noise = rng.normal(0,1,len(t))
                    noise[1:] -= .75*noise[:-1]
                    put(start+k*beat, noise*np.exp(-t*42), .027)
                for off in (0,.25):
                    t = np.arange(int(.06*sr))/sr
                    noise = rng.normal(0,1,len(t))
                    noise[1:] -= .95*noise[:-1]
                    put(start+k*beat+off, noise*np.exp(-t*70), .011, .35)
    for cut in [6,10,17,23,30,37,44,50,54]:
        note(cut, 2.4, [74,81,85], .045, .15, 3)
        length = .75
        t = np.arange(int(length*sr))/sr
        noise = rng.normal(0,1,len(t))
        # A quiet rising air texture gives each spatial edit a gentle accent.
        noise = np.convolve(noise, np.ones(32)/32, mode="same")
        put(cut-.55, noise*np.sin(np.pi*t/length)**2, .09, -.15)
    note(54, 5.5, [50,54,57,61,69], .14)
    t = np.arange(len(audio))/sr
    audio *= (np.minimum(t/1.5,1)*np.minimum((60-t)/2,1)).clip(0,1)[:,None]
    audio *= .84 / max(float(np.max(np.abs(audio))), .001)
    with wave.open(str(CACHE / "score.wav"), "wb") as f:
        f.setnchannels(2); f.setsampwidth(2); f.setframerate(sr)
        f.writeframes((np.clip(audio,-1,1)*32767).astype("<i2").tobytes())


def prepare():
    CACHE.mkdir(parents=True, exist_ok=True)
    OUT.mkdir(parents=True, exist_ok=True)
    qrcode.make(DOWNLOAD, box_size=7, border=4).save(CACHE / "qr.png")
    if not (CACHE / "score.wav").is_file():
        music()


def previews():
    sheet = Image.new("RGB", (1920, 2700), "#eef5ef")
    for i in range(10):
        frame = Image.open(CACHE / f"preview-{i}.jpg")
        sheet.paste(frame.resize((960,540), Image.Resampling.LANCZOS), ((i%2)*960,(i//2)*540))
    sheet.save(OUT / "NorthStar-Fluent-storyboard.jpg", quality=93)
    Image.open(CACHE / "preview-0.jpg").save(OUT / "NorthStar-Fluent-cover.jpg", quality=96)


def verify():
    video = OUT / "NorthStar-Fluent-60s-1080p.mp4"
    ffmpeg = imageio_ffmpeg.get_ffmpeg_exe()
    result = subprocess.run([ffmpeg,"-v","error","-i",str(video),"-f","null","-"],
                            capture_output=True, text=True, check=True)
    if result.stderr.strip():
        raise RuntimeError(result.stderr)
    frames = imageio_ffmpeg.read_frames(str(video))
    info = next(frames)
    count = sum(1 for _ in frames)
    assert info["size"] == (1920,1080), info
    assert info["fps"] == 30, info
    assert info["duration"] == 60, info
    assert count == 1800, count
    assert info["codec"] == "h264" and info["audio_codec"] == "aac", info
    shots = json.loads((OUT / "storyboard.json").read_text("utf-8"))
    assert shots[0]["start"] == 0 and shots[-1]["end"] == 60
    assert all(a["end"] == b["start"] for a,b in zip(shots,shots[1:]))
    def stamp(t):
        return f"00:{t//60:02d}:{t%60:02d},000"
    srt = []
    for i, shot in enumerate(shots):
        text = shot["title"].replace("\n", "") or "NorthStar Minecraft Launcher。下一站，你的世界。"
        srt.append(f"{i+1}\n{stamp(shot['start'])} --> {stamp(shot['end'])}\n{text}")
    (OUT / "NorthStar-Fluent.zh-CN.srt").write_text("\n\n".join(srt)+"\n", "utf-8")
    report = {"file": video.name,"duration_seconds":60,"resolution":[1920,1080],"fps":30,
              "decoded_frames":count,"video_codec":info["codec"],"audio_codec":info["audio_codec"],
              "full_decode":"passed","renderer":"Three.js 0.180.0 / Edge WebGL / FFmpeg",
              "download_url":DOWNLOAD,"sha256":hashlib.sha256(video.read_bytes()).hexdigest()}
    (OUT / "verification.json").write_text(json.dumps(report,ensure_ascii=False,indent=2)+"\n", "utf-8")
    print(json.dumps(report,ensure_ascii=False,indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prepare",action="store_true")
    parser.add_argument("--previews",action="store_true")
    parser.add_argument("--verify",action="store_true")
    args = parser.parse_args()
    if args.prepare: prepare()
    if args.previews: previews()
    if args.verify: verify()
