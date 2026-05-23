#!/usr/bin/env python3
import struct
import os
import sys
import math

def analyze_active(file_path):
    if not os.path.exists(file_path):
        print(f"Error: {file_path} not found")
        sys.exit(1)

    with open(file_path, 'rb') as f:
        data = f.read()
    
    sample_count = len(data) // 4
    samples = list(struct.unpack(f"{sample_count}f", data))
    
    # Slice active region (frame 180 to 300 -> approx indices 140000 to 220000)
    start_idx = 140000
    end_idx = min(220000, len(samples))
    
    active_samples = samples[start_idx:end_idx]
    active_len = len(active_samples)
    
    print(f"Analyzing active region: indices {start_idx} to {end_idx} ({active_len} samples, ~{active_len/44100:.3f}s)")
    
    if active_len == 0:
        print("Active region is empty.")
        return

    max_val = -float('inf')
    min_val = float('inf')
    sum_val = 0.0
    zero_count = 0
    
    for s in active_samples:
        if s > max_val:
            max_val = s
        if s < min_val:
            min_val = s
        sum_val += s
        if s == 0.0:
            zero_count += 1
            
    avg_val = sum_val / active_len
    print(f"  Min Value: {min_val:.6f}")
    print(f"  Max Value: {max_val:.6f}")
    print(f"  Average (DC): {avg_val:.6f}")
    print(f"  Zero samples in active region: {zero_count} ({zero_count / active_len * 100:.2f}%)")

    # Check for micro-flatlines (identical consecutive non-zero samples)
    # For high-frequency square waves, flatlines of 5-15 samples are normal (sequencer steps).
    # But very long flatlines might be an issue.
    flatline_len = 0
    max_flatline_len = 0
    flatline_val = 0.0
    flatline_streaks = []
    
    for i in range(1, len(active_samples)):
        s1 = active_samples[i-1]
        s2 = active_samples[i]
        if s1 == s2 and s1 != 0.0:
            flatline_len += 1
        else:
            if flatline_len > 0:
                flatline_streaks.append((flatline_len, s1))
                if flatline_len > max_flatline_len:
                    max_flatline_len = flatline_len
                    flatline_val = s1
            flatline_len = 0
            
    print(f"\n=== Active Flatline Detection ===")
    print(f"  Max identical consecutive non-zero samples: {max_flatline_len} (Value: {flatline_val:.6f})")
    
    # Count streaks longer than 100 samples (approx 2.2ms of constant value)
    long_streaks = [s for s in flatline_streaks if s[0] > 100]
    print(f"  Total non-zero flatline streaks > 100 samples (2.2ms): {len(long_streaks)}")
    for i, streak in enumerate(long_streaks[:5]):
        print(f"    Streak {i}: {streak[0]} samples of value {streak[1]:.6f}")

    # Check for high-frequency delta changes (checking for extremely sharp transitions)
    deltas = []
    for i in range(1, len(active_samples)):
        deltas.append(abs(active_samples[i] - active_samples[i-1]))
    
    max_d = max(deltas) if deltas else 0.0
    avg_d = sum(deltas)/len(deltas) if deltas else 0.0
    print(f"\n=== Delta Analysis ===")
    print(f"  Max Delta: {max_d:.6f}")
    print(f"  Avg Delta: {avg_d:.6f}")

if __name__ == "__main__":
    analyze_active("audio300.bin")
