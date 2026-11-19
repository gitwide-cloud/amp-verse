#!/usr/bin/env python3
"""
SAG-65 Deterministic Synthetic Transition Fixture Generator & Baseline DSP Verifier.

Generates isolated synthetic audio fixtures (WAV) and machine-readable JSON test vectors
for AMP Verse Auto DJ transition rules. Measures BOTH:
  (A) Current AMP Verse baseline behavior (exact formulas verified from origin/main aaec29c
      and origin/delivery/preview-packaging-followup-20261118 62195e6), and
  (B) Proposed SAG-65 transition contract behavior.

Safety invariants:
  - Operates strictly in the specified --out-dir (isolated temp/scratch directory).
  - Never reads or modifies any live library DB, playlists, cues, source audio, or playback.
  - Zero external Python dependencies (stdlib only + optional ffmpeg ebur128 verification).
"""

import argparse
import hashlib
import json
import math
import os
import re
import struct
import subprocess
import wave

SAMPLE_RATE = 44100


def sha256_file(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def write_stereo_wav(path: str, left: list[float], right: list[float], sr: int = SAMPLE_RATE) -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    n = min(len(left), len(right))
    raw = bytearray(n * 4)
    idx = 0
    for i in range(n):
        l = max(-1.0, min(1.0, left[i]))
        r = max(-1.0, min(1.0, right[i]))
        li = int(round(l * 32767.0))
        ri = int(round(r * 32767.0))
        struct.pack_into("<hh", raw, idx, li, ri)
        idx += 4
    with wave.open(path, "wb") as wf:
        wf.setnchannels(2)
        wf.setsampwidth(2)
        wf.setframerate(sr)
        wf.writeframes(bytes(raw))


def measure_ebur128(wav_path: str) -> dict:
    """Measure integrated LUFS, LRA, and True Peak (dBTP) via FFmpeg ITU-R BS.1770 ebur128."""
    proc = subprocess.run(
        [
            "ffmpeg", "-hide_banner", "-nostats", "-i", wav_path,
            "-af", "ebur128=peak=true", "-f", "null", "-"
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=True,
    )
    txt = proc.stderr
    summary_idx = txt.rfind("Summary:")
    summary = txt[summary_idx:] if summary_idx >= 0 else txt

    def grab(pattern: str):
        m = re.search(pattern, summary)
        return float(m.group(1)) if m else None

    return {
        "integrated_lufs": grab(r"I:\s*(-?[0-9]+(?:\.[0-9]+)?)\s*LUFS"),
        "lra_lu": grab(r"LRA:\s*(-?[0-9]+(?:\.[0-9]+)?)\s*LU"),
        "true_peak_dbtp": grab(r"Peak:\s*(-?[0-9]+(?:\.[0-9]+)?)\s*dBFS"),
    }


def synth_kick_track(
    duration_s: float,
    bpm: float,
    offset_s: float = 0.0,
    amp: float = 0.75,
    sub_hz: float = 55.0,
    chord_hz: float = 220.0,
    vocal_ranges: list[tuple[float, float]] | None = None,
    vocal_hz: float = 660.0,
    vocal_amp: float = 0.35,
    drift_bpm_end: float | None = None,
) -> dict[str, list[float]]:
    """Generate deterministic 3-stem (vocals, instruments, drums) + summed mono-compatible stereo."""
    n = int(round(duration_s * SAMPLE_RATE))
    drums = [0.0] * n
    inst = [0.0] * n
    voc = [0.0] * n

    # Generate beat timestamps (constant or linearly drifting BPM)
    beats = []
    t = offset_s
    b_idx = 0
    while t < duration_s:
        beats.append((b_idx, t))
        if drift_bpm_end is None:
            cur_bpm = bpm
        else:
            frac = max(0.0, min(1.0, t / max(duration_s, 1e-6)))
            cur_bpm = bpm + (drift_bpm_end - bpm) * frac
        t += 60.0 / cur_bpm
        b_idx += 1

    kick_len = int(0.14 * SAMPLE_RATE)
    for b_idx, bt in beats:
        start = int(round(bt * SAMPLE_RATE))
        downbeat = (b_idx % 4 == 0)
        phrase_start = (b_idx % 16 == 0)
        b_amp = amp * (1.0 if downbeat else 0.78)
        for k in range(kick_len):
            pos = start + k
            if pos >= n:
                break
            kt = k / SAMPLE_RATE
            env = math.exp(-kt * 26.0)
            freq = sub_hz + 95.0 * math.exp(-kt * 40.0)
            sample = b_amp * env * math.sin(2.0 * math.pi * freq * kt)
            if phrase_start and k < int(0.04 * SAMPLE_RATE):
                # Short crisp marker click on phrase downbeat for phase verification
                sample += 0.22 * amp * math.exp(-kt * 90.0) * math.sin(2.0 * math.pi * 1760.0 * kt)
            drums[pos] += sample

    # Harmonic pad in instruments stem
    pad_amp = amp * 0.32
    for i in range(n):
        ti = i / SAMPLE_RATE
        inst[i] = pad_amp * (
            0.65 * math.sin(2.0 * math.pi * chord_hz * ti)
            + 0.35 * math.sin(2.0 * math.pi * (chord_hz * 1.5) * ti)
        )

    # Vocal formant-like carrier in active ranges (300-3400 Hz band)
    if vocal_ranges:
        for vs, ve in vocal_ranges:
            i0 = max(0, int(round(vs * SAMPLE_RATE)))
            i1 = min(n, int(round(ve * SAMPLE_RATE)))
            for i in range(i0, i1):
                ti = i / SAMPLE_RATE
                vib = 1.0 + 0.015 * math.sin(2.0 * math.pi * 5.5 * ti)
                voc[i] = vocal_amp * (
                    0.6 * math.sin(2.0 * math.pi * vocal_hz * vib * ti)
                    + 0.4 * math.sin(2.0 * math.pi * (vocal_hz * 2.0) * vib * ti)
                )

    mix = [drums[i] + inst[i] + voc[i] for i in range(n)]
    return {"drums": drums, "instruments": inst, "vocals": voc, "mix": mix, "beats": [bt for _, bt in beats]}


def synth_flat_pad(duration_s: float, amp: float = 0.72, freq_hz: float = 180.0) -> list[float]:
    """Brickwalled low-crest ambient pad (high RMS, zero rhythmic transients)."""
    n = int(round(duration_s * SAMPLE_RATE))
    out = [0.0] * n
    for i in range(n):
        t = i / SAMPLE_RATE
        raw = math.sin(2.0 * math.pi * freq_hz * t) + 0.5 * math.sin(2.0 * math.pi * freq_hz * 1.498 * t)
        # Soft saturate to create low crest factor (~3 dB) like brickwalled ambient pad
        out[i] = amp * math.tanh(2.5 * raw)
    return out


def amp_current_rms_energy(samples: list[float]) -> float:
    """Exact formula from crates/amp-core/src/analysis.rs:241-244 & energy_profile.rs:131."""
    rms = math.sqrt(sum(x * x for x in samples) / max(1, len(samples)))
    val = (20.0 * math.log10(max(rms, 1e-6)) + 48.0) / 42.0
    return max(0.0, min(1.0, val))


def perceptual_energy_proxy(samples: list[float], lufs: float) -> dict:
    """
    Loudness-normalized perceptual energy proxy (proposed):
    Normalizes signal to -14 LUFS before computing transient/onset density and crest factor,
    so mastering loudness does not masquerade as musical energy.
    """
    gain = 10.0 ** ((-14.0 - lufs) / 20.0)
    block = 441  # 10 ms frames
    frame_rms = []
    peak = 0.0
    for i in range(0, len(samples) - block, block):
        s2 = 0.0
        for x in samples[i : i + block]:
            gx = x * gain
            ax = abs(gx)
            if ax > peak:
                peak = ax
            s2 += gx * gx
        frame_rms.append(math.sqrt(s2 / block))
    mean_rms = math.sqrt(sum(r * r for r in frame_rms) / max(1, len(frame_rms)))
    crest_db = 20.0 * math.log10(max(peak, 1e-6) / max(mean_rms, 1e-6))
    onsets = [max(0.0, frame_rms[i] - frame_rms[i - 1]) for i in range(1, len(frame_rms))]
    onset_peak = max(onsets) if onsets else 0.0
    # Count strong rhythmic onsets per second
    thresh = max(0.04, onset_peak * 0.25)
    hits = 0
    last_hit = -10
    for i, ov in enumerate(onsets):
        if ov >= thresh and (i - last_hit) >= 12:
            hits += 1
            last_hit = i
    duration_s = len(samples) / SAMPLE_RATE
    onset_rate_hz = hits / max(duration_s, 1e-6)
    # Composite normalized score [0..1]: transient activity + rhythmic pulse density
    flux_mean = sum(onsets) / max(1, len(onsets))
    score = max(0.0, min(1.0, 0.55 * min(1.0, onset_rate_hz / 3.5) + 0.45 * min(1.0, flux_mean / 0.06)))
    return {
        "normalized_crest_db": round(crest_db, 2),
        "onset_rate_hz": round(onset_rate_hz, 2),
        "mean_onset_flux": round(flux_mean, 5),
        "perceptual_energy_0_1": round(score, 3),
    }


def amp_current_equal_power(pos: float) -> tuple[float, float]:
    """Exact formula from crates/amp-core/src/show.rs:735-738."""
    p = max(0.0, min(1.0, pos)) * (math.pi / 2.0)
    return (math.cos(p), math.sin(p))


def amp_current_blend_stems(p: float) -> tuple[list[float], list[float]]:
    """Exact formula from apps/amp-player/src/performance.rs:46-55."""
    p = max(0.0, min(1.0, p))
    ramp = lambda a, b: max(0.0, min(1.0, (p - a) / (b - a)))
    drums = ramp(0.4, 0.6)
    out_w = [1.0 - ramp(0.0, 0.25), 1.0 - ramp(0.5, 1.0), 1.0 - drums]
    in_w = [ramp(0.1, 0.35), ramp(0.45, 0.85), drums]
    return out_w, in_w


def proposed_vocal_safe_blend_stems(p: float, out_has_vocal: bool, in_has_vocal: bool) -> tuple[list[float], list[float]]:
    """
    Proposed vocal-exclusive stem handoff:
    - If both tracks have active vocals in the transition window, outgoing vocal reaches 0.0
      BEFORE incoming vocal rises above 0.0 (zero dual-vocal overlap; 5% guard gap).
    - Drum swap snaps tightly across the midpoint bar boundary (0.48..0.52).
    """
    p = max(0.0, min(1.0, p))
    ramp = lambda a, b: max(0.0, min(1.0, (p - a) / (b - a)))
    drums = ramp(0.48, 0.52)
    if out_has_vocal and in_has_vocal:
        out_v = 1.0 - ramp(0.0, 0.35)
        in_v = ramp(0.42, 0.70)
    else:
        out_v = 1.0 - ramp(0.0, 0.40)
        in_v = ramp(0.45, 0.75)
    out_w = [out_v, 1.0 - ramp(0.50, 0.95), 1.0 - drums]
    in_w = [in_v, ramp(0.15, 0.60), drums]
    return out_w, in_w


def lookahead_peak_limiter(samples: list[float], ceiling: float = 0.89125, lookahead_ms: float = 5.0, release_ms: float = 60.0) -> list[float]:
    """
    Simple transparent lookahead limiter (-1.0 dBTP ~ 0.89125 linear ceiling).
    Zero hard-clipping; smooth attack over lookahead window and exponential release.
    """
    n = len(samples)
    la = max(1, int(round(lookahead_ms * SAMPLE_RATE / 1000.0)))
    rel_coeff = math.exp(-1.0 / max(1.0, release_ms * SAMPLE_RATE / 1000.0))
    att_coeff = math.exp(-1.0 / max(1.0, (lookahead_ms * 0.35) * SAMPLE_RATE / 1000.0))

    # Compute target gain per sample looking ahead `la` samples
    target_gain = [1.0] * n
    for i in range(n):
        hi = min(n, i + la)
        pk = 0.0
        for j in range(i, hi):
            ax = abs(samples[j])
            if ax > pk:
                pk = ax
        if pk > ceiling:
            target_gain[i] = ceiling / pk

    out = [0.0] * n
    g = 1.0
    for i in range(n):
        tg = target_gain[i]
        if tg < g:
            g = tg + (g - tg) * att_coeff
        else:
            g = tg + (g - tg) * rel_coeff
        # Safety envelope cap (smoothly reached before transient thanks to lookahead)
        if abs(samples[i]) * g > ceiling:
            g = ceiling / abs(samples[i])
        out[i] = samples[i] * g
    return out


def run_experiments(out_dir: str) -> dict:
    os.makedirs(out_dir, exist_ok=True)
    wav_dir = os.path.join(out_dir, "wav")
    os.makedirs(wav_dir, exist_ok=True)

    results = {
        "schema": "amp.sag65.synthetic_transition_fixtures.v1",
        "sample_rate": SAMPLE_RATE,
        "code_baselines_audited": {
            "origin_main": "aaec29c",
            "preview_baseline": "62195e6 (origin/delivery/preview-packaging-followup-20261118)",
        },
        "measured_baseline_vs_proposed": {},
        "fixtures": [],
    }

    # --- Experiment 1 (F10): Master Bus Clipping & Loudness Mismatch ---
    # Two hot correlated-bass tracks (~ -7 LUFS) crossfading at 124 BPM for 4.0 seconds
    hot_a = synth_kick_track(8.0, 124.0, offset_s=0.0, amp=0.92, sub_hz=55.0, chord_hz=220.0)
    hot_b = synth_kick_track(8.0, 124.0, offset_s=0.0, amp=0.92, sub_hz=55.0, chord_hz=220.0)
    # Saturate slightly to model modern mastered EDM (~ -7 LUFS)
    mix_a = [max(-0.99, min(0.99, math.tanh(1.6 * x))) for x in hot_a["mix"]]
    mix_b = [max(-0.99, min(0.99, math.tanh(1.6 * x))) for x in hot_b["mix"]]
    quiet_b = [0.38 * x for x in mix_b]  # ~ 8.4 dB quieter track (-15 LUFS)

    p_hot_a = os.path.join(wav_dir, "f10_hot_outgoing.wav")
    p_hot_b = os.path.join(wav_dir, "f10_hot_incoming.wav")
    p_quiet_b = os.path.join(wav_dir, "f10_quiet_incoming.wav")
    write_stereo_wav(p_hot_a, mix_a, mix_a)
    write_stereo_wav(p_hot_b, mix_b, mix_b)
    write_stereo_wav(p_quiet_b, quiet_b, quiet_b)

    ebur_hot_a = measure_ebur128(p_hot_a)
    ebur_hot_b = measure_ebur128(p_hot_b)
    ebur_quiet_b = measure_ebur128(p_quiet_b)

    # Render 4s crossfade under:
    # (1) Current radio.rs:547 (equal-power without -3 dB bus pad, hard clamp -1..1)
    # (2) Current mixer.rs:1179,1220 (equal-power with -3 dB pad, +3 dB trim/EQ boost, hard clamp -0.98..0.98)
    # (3) Proposed: -14 LUFS auto-gain + -3 dB summing headroom + -1.0 dBTP lookahead limiter
    n_xf = int(4.0 * SAMPLE_RATE)
    radio_cur = []
    radio_clipped = 0
    mixer_cur = []
    mixer_clipped = 0
    proposed_bus = []
    inv_sqrt2 = 1.0 / math.sqrt(2.0)

    gain_a_norm = 10.0 ** ((-14.0 - ebur_hot_a["integrated_lufs"]) / 20.0)
    gain_b_norm = 10.0 ** ((-14.0 - ebur_hot_b["integrated_lufs"]) / 20.0)
    gain_qb_norm = 10.0 ** ((-14.0 - ebur_quiet_b["integrated_lufs"]) / 20.0)

    for i in range(n_xf):
        pos = i / max(1, n_xf - 1)
        w0, w1 = amp_current_equal_power(pos)
        xa, xb = mix_a[i], mix_b[i]

        # radio.rs:547 exact formula
        r_raw = xa * w0 + xb * w1
        if abs(r_raw) > 1.0:
            radio_clipped += 1
        radio_cur.append(max(-1.0, min(1.0, r_raw)))

        # mixer.rs:1179 + 1220 with typical +3 dB trim/EQ boost on a loud club master
        m_raw = (xa * 1.4125 * w0 + xb * 1.4125 * w1) * inv_sqrt2
        if abs(m_raw) > 0.98:
            mixer_clipped += 1
        mixer_cur.append(max(-0.98, min(0.98, m_raw)))

        # Proposed: ITU-R BS.1770 auto-gain to -14 LUFS + equal-power + lookahead limiter
        p_raw = (xa * gain_a_norm) * w0 + (xb * gain_b_norm) * w1
        proposed_bus.append(p_raw)

    proposed_limited = lookahead_peak_limiter(proposed_bus, ceiling=0.89125)
    proposed_clipped = sum(1 for x in proposed_limited if abs(x) >= 0.999)

    p_xf_radio = os.path.join(wav_dir, "f10_xfade_current_radio.wav")
    p_xf_mixer = os.path.join(wav_dir, "f10_xfade_current_mixer_plus3db.wav")
    p_xf_prop = os.path.join(wav_dir, "f10_xfade_proposed_autogain_limiter.wav")
    write_stereo_wav(p_xf_radio, radio_cur, radio_cur)
    write_stereo_wav(p_xf_mixer, mixer_cur, mixer_cur)
    write_stereo_wav(p_xf_prop, proposed_limited, proposed_limited)

    results["measured_baseline_vs_proposed"]["loudness_and_headroom_f10"] = {
        "source_hot_outgoing_lufs": ebur_hot_a["integrated_lufs"],
        "source_quiet_incoming_lufs": ebur_quiet_b["integrated_lufs"],
        "uncorrected_loudness_jump_lu": round(ebur_hot_a["integrated_lufs"] - ebur_quiet_b["integrated_lufs"], 2),
        "proposed_autogain_db_for_quiet_incoming": round(20.0 * math.log10(gain_qb_norm), 2),
        "current_radio_rs_clipped_samples": radio_clipped,
        "current_radio_rs_clipped_pct": round(100.0 * radio_clipped / n_xf, 3),
        "current_radio_rs_ebur128": measure_ebur128(p_xf_radio),
        "current_mixer_rs_plus3db_clipped_samples": mixer_clipped,
        "current_mixer_rs_plus3db_clipped_pct": round(100.0 * mixer_clipped / n_xf, 3),
        "current_mixer_rs_plus3db_ebur128": measure_ebur128(p_xf_mixer),
        "proposed_autogain_limiter_clipped_samples": proposed_clipped,
        "proposed_autogain_limiter_ebur128": measure_ebur128(p_xf_prop),
    }

    # --- Experiment 2 (F05): Vocal Clash Overlap Measurement ---
    v_out = synth_kick_track(6.0, 124.0, vocal_ranges=[(0.0, 4.0)], vocal_hz=520.0, vocal_amp=0.45)
    v_in = synth_kick_track(6.0, 124.0, vocal_ranges=[(0.0, 5.0)], vocal_hz=680.0, vocal_amp=0.45)
    cur_dual_vocal_frames = 0
    prop_dual_vocal_frames = 0
    cur_dual_vocal_rms_acc = 0.0
    prop_dual_vocal_rms_acc = 0.0
    n_v = int(4.0 * SAMPLE_RATE)
    for i in range(n_v):
        p = i / max(1, n_v - 1)
        co, ci = amp_current_blend_stems(p)
        po, pi = proposed_vocal_safe_blend_stems(p, True, True)
        ov_c = abs(v_out["vocals"][i] * co[0])
        iv_c = abs(v_in["vocals"][i] * ci[0])
        if co[0] > 0.05 and ci[0] > 0.05:
            cur_dual_vocal_frames += 1
            cur_dual_vocal_rms_acc += min(ov_c, iv_c) ** 2
        ov_p = abs(v_out["vocals"][i] * po[0])
        iv_p = abs(v_in["vocals"][i] * pi[0])
        if po[0] > 0.05 and pi[0] > 0.05:
            prop_dual_vocal_frames += 1
            prop_dual_vocal_rms_acc += min(ov_p, iv_p) ** 2

    results["measured_baseline_vs_proposed"]["vocal_clash_f05"] = {
        "transition_seconds": 4.0,
        "current_blend_stems_dual_vocal_overlap_ms": round(1000.0 * cur_dual_vocal_frames / SAMPLE_RATE, 1),
        "current_blend_stems_dual_vocal_overlap_pct": round(100.0 * cur_dual_vocal_frames / n_v, 2),
        "current_blend_stems_min_overlap_rms": round(math.sqrt(cur_dual_vocal_rms_acc / max(1, cur_dual_vocal_frames)), 4),
        "proposed_vocal_safe_dual_vocal_overlap_ms": round(1000.0 * prop_dual_vocal_frames / SAMPLE_RATE, 1),
        "proposed_vocal_safe_dual_vocal_overlap_pct": round(100.0 * prop_dual_vocal_frames / n_v, 2),
    }

    # --- Experiment 3 (F07): Energy Metric Loudness-War Trap ---
    dynamic_dance = synth_kick_track(8.0, 126.0, amp=0.65, sub_hz=58.0, chord_hz=261.63)["mix"]
    brickwalled_pad = synth_flat_pad(8.0, amp=0.62, freq_hz=196.0)
    p_dyn = os.path.join(wav_dir, "f07_dynamic_dance.wav")
    p_pad = os.path.join(wav_dir, "f07_brickwalled_ambient_pad.wav")
    write_stereo_wav(p_dyn, dynamic_dance, dynamic_dance)
    write_stereo_wav(p_pad, brickwalled_pad, brickwalled_pad)
    ebur_dyn = measure_ebur128(p_dyn)
    ebur_pad = measure_ebur128(p_pad)

    results["measured_baseline_vs_proposed"]["energy_loudness_trap_f07"] = {
        "dynamic_dance_lufs": ebur_dyn["integrated_lufs"],
        "brickwalled_ambient_pad_lufs": ebur_pad["integrated_lufs"],
        "current_raw_rms_energy_dynamic_dance": round(amp_current_rms_energy(dynamic_dance), 3),
        "current_raw_rms_energy_brickwalled_pad": round(amp_current_rms_energy(brickwalled_pad), 3),
        "current_engine_misranks_pad_higher_than_dance": amp_current_rms_energy(brickwalled_pad) > amp_current_rms_energy(dynamic_dance),
        "proposed_perceptual_dynamic_dance": perceptual_energy_proxy(dynamic_dance, ebur_dyn["integrated_lufs"]),
        "proposed_perceptual_brickwalled_pad": perceptual_energy_proxy(brickwalled_pad, ebur_pad["integrated_lufs"]),
    }

    # --- Experiment 4 (F01 / F02 / F08): Phase Alignment & Drift Diagnostics ---
    # Generate F01 steady 124 BPM pair & F08 drifting 118->122 BPM track
    f01_a = synth_kick_track(16.0, 124.0, offset_s=0.10)
    f01_b = synth_kick_track(16.0, 124.0, offset_s=0.25)
    f08_drift = synth_kick_track(32.0, 118.0, offset_s=0.10, drift_bpm_end=122.0)
    p_f01_a = os.path.join(wav_dir, "f01_steady_124a.wav")
    p_f01_b = os.path.join(wav_dir, "f01_steady_124b.wav")
    p_f08 = os.path.join(wav_dir, "f08_drifting_118_to_122.wav")
    write_stereo_wav(p_f01_a, f01_a["mix"], f01_a["mix"])
    write_stereo_wav(p_f01_b, f01_b["mix"], f01_b["mix"])
    write_stereo_wav(p_f08, f08_drift["mix"], f08_drift["mix"])

    # Compute max constant-grid phase deviation on F08 vs mean BPM 120.0
    mean_period = 60.0 / 120.0
    max_phase_err_beats = 0.0
    for bt in f08_drift["beats"]:
        ph = ((bt - 0.10) / mean_period) % 1.0
        err = min(ph, 1.0 - ph)
        if err > max_phase_err_beats:
            max_phase_err_beats = err

    results["measured_baseline_vs_proposed"]["drift_and_phase_f01_f08"] = {
        "f01_beat_count": len(f01_a["beats"]),
        "f08_beat_count": len(f08_drift["beats"]),
        "f08_start_bpm": 118.0,
        "f08_end_bpm": 122.0,
        "f08_max_constant_grid_phase_error_beats": round(max_phase_err_beats, 3),
        "f08_exceeds_stability_threshold_0_12": max_phase_err_beats > 0.12,
    }

    # --- Construct Complete 12-Fixture Contract Test Vector Matrix ---
    results["fixtures"] = [
        {
            "id": "F01",
            "name": "phrase_downbeat_aligned_124bpm",
            "category": "phrase_downbeat_alignment",
            "outgoing": {"bpm": 124.0, "beat_offset": 0.10, "mix_bar_anchor": 0.10, "mix_phrase_anchor": 0.10, "tempo_confidence": 0.94, "rhythm_stable": True, "harmonic_key": "8A", "key_confidence": 0.88, "lufs": -14.0, "wav": "wav/f01_steady_124a.wav", "sha256": sha256_file(p_f01_a)},
            "incoming": {"bpm": 124.0, "beat_offset": 0.25, "mix_bar_anchor": 0.25, "mix_phrase_anchor": 0.25, "tempo_confidence": 0.93, "rhythm_stable": True, "harmonic_key": "9A", "key_confidence": 0.85, "lufs": -14.0, "wav": "wav/f01_steady_124b.wav", "sha256": sha256_file(p_f01_b)},
            "expected_tier": "T0_PHRASE_LOCKED",
            "expected_rate": 1.0,
            "expected_pulse_factor": 1.0,
            "max_phase_error_ms": 1.0,
            "expected_fallback_reason": None,
        },
        {
            "id": "F02",
            "name": "unreviewed_phrase_trap_124bpm",
            "category": "phrase_downbeat_alignment",
            "outgoing": {"bpm": 124.0, "beat_offset": 0.10, "mix_bar_anchor": None, "mix_phrase_anchor": None, "downbeat_verified": False, "tempo_confidence": 0.85, "rhythm_stable": True},
            "incoming": {"bpm": 124.0, "beat_offset": 0.25, "mix_bar_anchor": None, "mix_phrase_anchor": None, "downbeat_verified": False, "tempo_confidence": 0.85, "rhythm_stable": True},
            "requested_timing": "Phrase16",
            "current_engine_bug": "phrase_anchor() silently falls back to beat_offset (mix_schedule.rs:72), counting 16 beats from arbitrary beat 0",
            "expected_tier": "T2_BEAT_LOCKED",
            "expected_fallback_reason": "unverified_phrase_anchor_downgraded_to_beat",
        },
        {
            "id": "F03",
            "name": "half_double_tempo_140_to_70",
            "category": "half_double_tempo",
            "outgoing": {"bpm": 140.0, "beat_offset": 0.0, "tempo_confidence": 0.90, "rhythm_stable": True, "harmonic_key": "5A", "key_confidence": 0.80},
            "incoming": {"bpm": 70.0, "beat_offset": 0.0, "tempo_confidence": 0.88, "rhythm_stable": True, "harmonic_key": "5A", "key_confidence": 0.80},
            "requested_timing": "Phrase16",
            "expected_tier": "T2_BEAT_LOCKED",
            "expected_rate": 1.0,
            "expected_pulse_factor": 2.0,
            "expected_half_double_soft_penalty_gt": 0.0,
            "expected_fallback_reason": "half_double_pulse_ambiguous_phrase_downgraded_to_beat",
        },
        {
            "id": "F04",
            "name": "harmonic_soft_constraint_and_keylock_shift",
            "category": "harmonic_compatibility",
            "subcases": [
                {"case": "4a_same_bpm_adjacent_camelot", "out_key": "8A", "in_key": "9A", "in_rate": 1.0, "key_lock": False, "in_key_conf": 0.85, "expected_harmonic_cost_range": [0.25, 0.50]},
                {"case": "4b_plus6pct_no_keylock_semitone_clash", "out_key": "8A", "in_key": "8A", "in_rate": 1.06, "key_lock": False, "in_key_conf": 0.85, "effective_sounding_key": "3A", "expected_harmonic_cost_min": 1.8},
                {"case": "4c_plus6pct_with_keylock_stays_8A", "out_key": "8A", "in_key": "8A", "in_rate": 1.06, "key_lock": True, "in_key_conf": 0.85, "effective_sounding_key": "8A", "expected_harmonic_cost_max": 0.1},
                {"case": "4d_low_key_confidence_attenuated", "out_key": "8A", "in_key": "2A", "in_rate": 1.0, "key_lock": True, "in_key_conf": 0.08, "expected_confidence_scaled_below_confident_clash": True},
                {"case": "4e_soft_constraint_never_hard_blocks_single_candidate", "candidates": 1, "out_key": "8A", "in_key": "2A", "expected_playable": True, "expected_mitigation": "shorter_fade_or_eq_filter_swap"},
            ],
        },
        {
            "id": "F05",
            "name": "vocal_clash_stem_and_nonstem",
            "category": "vocal_clashes",
            "outgoing_vocal_window_s": [0.0, 4.0],
            "incoming_vocal_window_s": [0.0, 5.0],
            "stems_loaded_mode": {"expected_dual_vocal_overlap_ms": 0.0, "expected_guard_gap_pct": 7.0},
            "non_stem_mode": {"expected_action": "shift_entry_or_exit_to_instrumental_or_apply_mid_duck_short_fade", "max_unattenuated_dual_vocal_s": 1.0},
        },
        {
            "id": "F06",
            "name": "stem_handoff_bleed_and_mute_preservation",
            "category": "stem_aware_handoffs",
            "user_stem_gains_outgoing": [1.0, 0.8, 1.0],
            "user_stem_gains_incoming": [0.0, 1.0, 0.9],
            "expected_invariants": [
                "incoming_vocal_remains_exactly_0_throughout_and_after_transition",
                "user_stem_gains_restored_exactly_on_complete_or_cancel",
                "drum_swap_window_le_1_beat_aligned_to_bar_or_beat",
                "missing_stems_on_either_deck_falls_back_to_eq_bass_swap",
            ],
        },
        {
            "id": "F07",
            "name": "energy_arc_loudness_trap",
            "category": "energy_arcs",
            "dynamic_dance_wav": "wav/f07_dynamic_dance.wav",
            "brickwalled_pad_wav": "wav/f07_brickwalled_ambient_pad.wav",
            "expected_invariants": [
                "perceptual_energy(dynamic_dance) > perceptual_energy(brickwalled_pad)",
                "multi_phase_arc_supports_warmup_build_peak_valley_cooldown",
                "show_ranking_does_not_double_count_energy_or_tempo",
            ],
        },
        {
            "id": "F08",
            "name": "dynamic_tempo_drift_live_drummer",
            "category": "dynamic_tempo",
            "wav": "wav/f08_drifting_118_to_122.wav",
            "sha256": sha256_file(p_f08),
            "start_bpm": 118.0,
            "end_bpm": 122.0,
            "expected_constant_grid_stable": False,
            "expected_tier_without_warp_anchors": "T4_UNSYNCED_CLEAN_FADE",
            "expected_fallback_reason": "rhythm_drift_exceeds_0_12_beats",
        },
        {
            "id": "F09",
            "name": "tempo_ramp_recovery_118_to_126",
            "category": "dynamic_tempo",
            "outgoing_bpm": 118.0,
            "incoming_bpm": 126.0,
            "initial_matched_rate": round(118.0 / 126.0, 6),
            "expected_post_handoff_behavior": "slew_incoming_rate_toward_1_0_at_le_0_25_pct_per_sec_with_key_lock",
            "anti_pattern_prevented": "permanent_tempo_gravity_locking_set_at_118_bpm",
        },
        {
            "id": "F10",
            "name": "loudness_jump_and_master_clipping",
            "category": "loudness_limiter_headroom",
            "hot_outgoing_wav": "wav/f10_hot_outgoing.wav",
            "hot_incoming_wav": "wav/f10_hot_incoming.wav",
            "quiet_incoming_wav": "wav/f10_quiet_incoming.wav",
            "expected_invariants": [
                "autogain_reduces_8_4_lu_jump_to_within_1_0_lu",
                "master_bus_true_peak_le_-1_0_dbtp",
                "zero_hard_clipped_samples_at_full_scale",
                "autogain_clamped_to_[-12_db, +6_db]_to_prevent_noise_floor_pumping",
            ],
        },
        {
            "id": "F11",
            "name": "low_confidence_sparse_grid_fallback",
            "category": "confidence_and_fallback",
            "subcases": [
                {"case": "11a_rhythm_stable_null_windows_lt_3", "bpm": 122.0, "beat_offset": 0.1, "tempo_confidence": 0.55, "rhythm_windows": 2, "rhythm_stable": None, "current_bug": "timing_reliable treats stable:null as reliable", "expected_tier": "T3_TEMPO_ONLY_OR_T4_UNSYNCED"},
                {"case": "11b_tempo_conf_0_42", "bpm": 122.0, "beat_offset": 0.1, "tempo_confidence": 0.42, "rhythm_stable": True, "expected_tier": "T4_UNSYNCED_CLEAN_FADE", "expected_rhythmic_fx_allowed": False},
                {"case": "11c_no_bpm_metadata", "bpm": None, "beat_offset": None, "expected_tier": "T4_UNSYNCED_CLEAN_FADE", "expected_no_fake_120bpm_grain": True},
            ],
        },
        {
            "id": "F12",
            "name": "short_tail_and_manual_cue_preservation",
            "category": "safety_and_fallback",
            "outgoing_remaining_s": 2.0,
            "requested_fade_s": 8.0,
            "manual_override": True,
            "manual_cues": [{"id": "user-drop", "seconds": 32.0, "kind": "hotcue"}],
            "expected_effective_fade_s": 1.85,
            "expected_tier": "T5_EMERGENCY_TAIL_RESCUE",
            "expected_manual_metadata_mutated": False,
        },
    ]

    manifest_path = os.path.join(out_dir, "sag65_synthetic_transition_fixtures.json")
    with open(manifest_path, "w", encoding="utf-8") as f:
        json.dump(results, f, indent=2)
    return results


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--out-dir", required=True)
    args = parser.parse_args()
    res = run_experiments(args.out_dir)
    print(json.dumps(res["measured_baseline_vs_proposed"], indent=2))
