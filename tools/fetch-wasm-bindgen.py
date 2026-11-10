#!/usr/bin/env python3
"""Fetch the exact lockfile CLI as a hash-pinned task/CI-local executable."""
from pathlib import Path
import argparse,hashlib,json,tarfile,urllib.request
PIN={'0.2.130':'9687f3bb551b9f66f37ed32a40d7913a00f7962db653054e56857bc306c3227f'}
parser=argparse.ArgumentParser();parser.add_argument('--lockfile',type=Path,required=True);parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
version=args.lockfile.read_text().split('name = "wasm-bindgen"\nversion = "')[1].split('"')[0]
if version not in PIN:raise SystemExit(f'Add the official release archive SHA256 for wasm-bindgen {version} before testing')
name=f'wasm-bindgen-{version}-x86_64-unknown-linux-musl';url=f'https://github.com/wasm-bindgen/wasm-bindgen/releases/download/{version}/{name}.tar.gz';args.output.mkdir(parents=True,exist_ok=True);archive=args.output/(name+'.tar.gz')
if not archive.exists():
 with urllib.request.urlopen(url,timeout=60)as response,archive.open('wb')as output:
  while block:=response.read(1024*1024):output.write(block)
actual=hashlib.sha256(archive.read_bytes()).hexdigest()
if actual!=PIN[version]:raise SystemExit('wasm-bindgen release archive checksum mismatch')
with tarfile.open(archive)as tar:
 for member in tar.getmembers():
  if not (args.output/member.name).resolve().is_relative_to(args.output.resolve()):raise SystemExit('unsafe archive path')
 tar.extractall(args.output)
print(args.output/name/'wasm-bindgen')
