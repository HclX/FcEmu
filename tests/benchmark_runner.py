#!/usr/bin/env python3
import subprocess
import socket
import time
import sys

class SimpleClient:
    def __init__(self, host, port, path):
        self.host = host
        self.port = port
        self.path = path
        self.sock = None

    def connect(self):
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.sock.settimeout(3.0)
        self.sock.connect((self.host, self.port))
        
        key = "dGhlIHNhbXBsZSBub25jZQ=="
        handshake = (
            f"GET {self.path} HTTP/1.1\r\n"
            f"Host: {self.host}:{self.port}\r\n"
            "Upgrade: websocket\r\n"
            "Connection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\n"
            "Sec-WebSocket-Version: 13\r\n\r\n"
        )
        self.sock.sendall(handshake.encode("utf-8"))
        
        resp = b""
        while b"\r\n\r\n" not in resp:
            chunk = self.sock.recv(1024)
            if not chunk:
                raise ConnectionError("WebSocket handshake failed")
            resp += chunk

    def send_frame(self, payload: bytes):
        opcode = 0x02
        length = len(payload)
        header = bytearray([0x80 | opcode])
        header.append(0x80 | length)
        mask_key = b"\x12\x34\x56\x78"
        header.extend(mask_key)
        
        masked = bytearray(length)
        for i in range(length):
            masked[i] = payload[i] ^ mask_key[i % 4]
        self.sock.sendall(header + masked)

    def recv_frame(self):
        header = self.sock.recv(2)
        if not header or len(header) < 2:
            return None
        payload_len = header[1] & 0x7F
        if payload_len == 126:
            len_bytes = self.sock.recv(2)
            payload_len = int.from_bytes(len_bytes, byteorder='big')
        elif payload_len == 127:
            len_bytes = self.sock.recv(8)
            payload_len = int.from_bytes(len_bytes, byteorder='big')
        
        masked = (header[1] & 0x80) != 0
        if masked:
            self.sock.recv(4) # ignore mask key
        
        payload = b""
        while len(payload) < payload_len:
            chunk = self.sock.recv(payload_len - len(payload))
            if not chunk:
                break
            payload += chunk
        return payload

    def close(self):
        if self.sock:
            self.sock.close()

def run_benchmark(bin_path, raw_mode):
    path = "/ws/stream?raw=true" if raw_mode else "/ws/stream"
    print(f"Starting server {bin_path}...")
    server_proc = subprocess.Popen(
        [bin_path, "--port", "8080"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True
    )
    time.sleep(1.5) # wait for server to start

    diagnostics_line = None
    try:
        client = SimpleClient("127.0.0.1", 8080, path)
        client.connect()
        print("Client connected, streaming 75 frames...")
        for _ in range(75):
            client.send_frame(bytes([0x00])) # send dummy controller input
            client.recv_frame()
        client.close()
    except Exception as e:
        print(f"Error during streaming: {e}")
    finally:
        server_proc.terminate()
        try:
            stdout, stderr = server_proc.communicate(timeout=5)
            for line in stdout.splitlines():
                if "[Diagnostics]" in line:
                    diagnostics_line = line
                    print(f"Found: {line}")
        except subprocess.TimeoutExpired:
            server_proc.kill()
            stdout, stderr = server_proc.communicate()
            for line in stdout.splitlines():
                if "[Diagnostics]" in line:
                    diagnostics_line = line
                    print(f"Found: {line}")
                    
    return diagnostics_line

def main():
    configs = [
        ("./target/debug/fce_web_server", False, "Debug (JPEG Mode)"),
        ("./target/debug/fce_web_server", True, "Debug (Raw RGB24 Zero-Copy Mode)"),
        ("./target/release/fce_web_server", False, "Release (JPEG Mode)"),
        ("./target/release/fce_web_server", True, "Release (Raw RGB24 Zero-Copy Mode)"),
    ]
    
    results = {}
    for bin_path, raw_mode, name in configs:
        print(f"\n=== Benchmarking {name} ===")
        line = run_benchmark(bin_path, raw_mode)
        results[name] = line
        
    print("\n=============================================")
    print("SUMMARY OF BENCHMARK RESULTS:")
    print("=============================================")
    for name, line in results.items():
        print(f"{name}: {line}")

if __name__ == "__main__":
    main()
