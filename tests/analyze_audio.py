#!/usr/bin/env python3
import struct
import os
import sys
import math

def analyze_audio(file_path):
    if not os.path.exists(file_path):
        print(f"Error: Audio file not found at {file_path}")
        sys.exit(1)

    file_size = os.path.getsize(file_path)
    sample_count = file_size // 4
    print(f"Analyzing {file_path}:")
    print(f"  File Size: {file_size} bytes")
    print(f"  Total f32 Samples: {sample_count}")
    
    if sample_count == 0:
        print("Error: Empty audio file.")
        return

    with open(file_path, 'rb') as f:
        data = f.read()
    
    # Unpack floats
    samples = list(struct.unpack(f"{sample_count}f", data))
    
    # 1. Basic stats
    nan_count = 0
    inf_count = 0
    max_val = -float('inf')
    min_val = float('inf')
    sum_val = 0.0
    
    for s in samples:
        if math.isnan(s):
            nan_count += 1
            continue
        if math.isinf(s):
            inf_count += 1
            continue
        
        if s > max_val:
            max_val = s
        if s < min_val:
            min_val = s
        sum_val += s
        
    avg_val = sum_val / (sample_count - nan_count - inf_count) if sample_count > (nan_count + inf_count) else 0.0
    
    print(f"\n=== Basic Statistics ===")
    print(f"  NaN count: {nan_count}")
    print(f"  Inf count: {inf_count}")
    print(f"  Min Value: {min_val:.6f}")
    print(f"  Max Value: {max_val:.6f}")
    print(f"  Average (DC Offset): {avg_val:.6f}")
    
    # 2. Clipping detection (exceeding -1.0 or 1.0)
    clipping_count = 0
    for s in samples:
        if not math.isnan(s) and not math.isinf(s):
            if s > 1.0 or s < -1.0:
                clipping_count += 1
                
    print(f"\n=== Clipping Detection ===")
    print(f"  Samples exceeding [-1.0, 1.0]: {clipping_count} ({clipping_count / sample_count * 100:.2f}%)")

    # 3. Discontinuity & Sudden Jumps Detection
    # Find the maximum delta between consecutive samples
    max_delta = 0.0
    discontinuity_threshold = 0.4 # Jumps of > 0.4 in a single sample
    discontinuity_count = 0
    
    for i in range(1, len(samples)):
        s1 = samples[i-1]
        s2 = samples[i]
        if math.isnan(s1) or math.isnan(s2) or math.isinf(s1) or math.isinf(s2):
            continue
        delta = abs(s2 - s1)
        if delta > max_delta:
            max_delta = delta
        if delta > discontinuity_threshold:
            discontinuity_count += 1
            if discontinuity_count < 10: # Print first few jumps
                print(f"  Discontinuity jump at index {i}: {s1:.4f} -> {s2:.4f} (delta: {delta:.4f})")
                
    print(f"\n=== Discontinuity & Sudden Jumps ===")
    print(f"  Maximum consecutive delta: {max_delta:.6f}")
    print(f"  Total sudden jumps (> {discontinuity_threshold}): {discontinuity_count}")

    # 4. Constant DC flatlines detection
    # Look for long stretches of identical non-zero samples
    flatline_len = 0
    max_flatline_len = 0
    flatline_val = 0.0
    consecutive_zero_count = 0
    
    for i in range(1, len(samples)):
        s1 = samples[i-1]
        s2 = samples[i]
        if s1 == s2:
            flatline_len += 1
            if s1 == 0.0:
                consecutive_zero_count += 1
        else:
            if flatline_len > max_flatline_len:
                max_flatline_len = flatline_len
                flatline_val = s1
            flatline_len = 0
            
    print(f"\n=== Flatline Detection ===")
    print(f"  Max identical consecutive samples: {max_flatline_len} (Value: {flatline_val:.6f})")
    print(f"  Total zero samples: {consecutive_zero_count} ({consecutive_zero_count / sample_count * 100:.2f}%)")

if __name__ == "__main__":
    file_path = "audio300.bin"
    if len(sys.argv) > 1:
        file_path = sys.argv[1]
    analyze_audio(file_path)
