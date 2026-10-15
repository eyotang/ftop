#!/usr/bin/env python3
"""Measure a Linux native UI process, including all its threads, without sampling tools."""
import argparse
import json
import os
import pathlib
import subprocess
import time

parser = argparse.ArgumentParser()
parser.add_argument('--binary', required=True)
parser.add_argument('--seconds', type=float, default=15)
parser.add_argument('--warmup', type=float, default=5)
parser.add_argument('--output', type=pathlib.Path)
parser.add_argument('--label', default='native')
parser.add_argument('--minimized', action='store_true')
parser.add_argument('--gpu', choices=['on','off'],default='on')
parser.add_argument('--motion', choices=['on','off'],default='off')
parser.add_argument('--backend', choices=['wayland', 'x11'], default='wayland')
args = parser.parse_args()
if args.seconds <= 0 or args.warmup < 0:
    parser.error('duration must be positive and warmup nonnegative')
if args.minimized and args.warmup < 4:
    parser.error('minimized measurement needs at least 4 seconds of warmup')
if not pathlib.Path('/proc/self/stat').exists():
    parser.error('This measurement harness requires Linux /proc')
environment = os.environ.copy()
if args.backend == 'x11':
    environment['WAYLAND_DISPLAY'] = ''
    environment['XMODIFIERS'] = '@im=none'
command=[str(pathlib.Path(args.binary).resolve()), '--motion' if args.motion == 'on' else '--no-motion']
if args.gpu == 'off':
    command.append('--no-gpu')
if args.minimized:
    command.append('--benchmark-hidden')
process = subprocess.Popen(command, env=environment)
try:
    time.sleep(args.warmup)
    if process.poll() is not None:
        raise SystemExit(f'Native application exited before measurement: {process.returncode}')
    stat = pathlib.Path(f'/proc/{process.pid}/stat')
    def counters():
        # comm can contain spaces and parentheses; the last ')' ends that field.
        values = stat.read_text().rsplit(')', 1)[1].split()
        return (int(values[11]) + int(values[12])) / os.sysconf('SC_CLK_TCK')
    def thread_counters():
        output = {}
        for task in pathlib.Path(f'/proc/{process.pid}/task').iterdir():
            try:
                text = (task / 'stat').read_text()
                fields = text.rsplit(')', 1)[1].split()
                output[task.name] = (text.split('(', 1)[1].rsplit(')', 1)[0], (int(fields[11]) + int(fields[12])) / os.sysconf('SC_CLK_TCK'))
            except FileNotFoundError:
                pass
        return output
    before_threads = thread_counters()
    before = counters()
    started = time.monotonic()
    time.sleep(args.seconds)
    after = counters()
    elapsed = time.monotonic() - started
    after_threads = thread_counters()
    status = pathlib.Path(f'/proc/{process.pid}/status').read_text().splitlines()
    rss = next(int(line.split()[1]) for line in status if line.startswith('VmRSS:'))
    one_core = 100 * (after - before) / elapsed
    result = {'label': args.label, 'backend': args.backend, 'motion': args.motion,
        'gpu': args.gpu, 'minimized': args.minimized, 'seconds': round(elapsed, 3), 'cpu_one_core_percent': round(one_core, 3), 'cpu_whole_machine_percent': round(one_core / os.cpu_count(), 3), 'logical_cpus': os.cpu_count(), 'rss_mib': round(rss / 1024, 2)}
    result['busiest_threads'] = sorted([{'name': name, 'cpu_one_core_percent': round(100 * (ticks - before_threads.get(tid, ('', 0))[1]) / elapsed, 3)} for tid, (name, ticks) in after_threads.items()], key=lambda item: -item['cpu_one_core_percent'])[:5]
    rendered = json.dumps(result, indent=2)
    print(rendered)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered + '\n')
finally:
    process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()
