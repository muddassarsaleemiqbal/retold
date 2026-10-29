"""Runs retold in a pseudo-terminal, types scripted keys and prints everything drawn."""
import fcntl, json, os, pty, shutil, signal, struct, subprocess, sys, termios, threading, time

bin_path = os.environ['RETOLD_BIN']
library = os.environ['RETOLD_LIB']
config = os.environ['RETOLD_CFG']
actions = os.environ.get('RETOLD_ACTIONS', '').split(',')

master, slave = pty.openpty()
original_attributes = termios.tcgetattr(slave)
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 30, 110, 0, 0))
pid = os.fork()
if pid == 0:
    os.close(master)
    os.environ['TERM'] = 'xterm-256color'
    if os.environ.get('RETOLD_PTY_DISCOVER_LIBRARY'):
        os.environ.pop('RETOLD_LIBRARY', None)
    else:
        os.environ['RETOLD_LIBRARY'] = library
    os.environ['RETOLD_CONFIG_DIR'] = config
    if 'RETOLD_TEST_PATH' in os.environ:
        os.environ['PATH'] = os.environ['RETOLD_TEST_PATH']
    os.dup2(slave, 0)
    os.dup2(slave, 1)
    os.dup2(slave, 2)
    os.close(slave)
    arguments = json.loads(os.environ.get('RETOLD_PTY_ARGS', '[]'))
    os.execv(bin_path, ['retold', *arguments])
else:
    chunks = []
    def reader():
        while True:
            try:
                data = os.read(master, 1024)
                if not data: break
                chunks.append(data)
            except OSError:
                break
    t = threading.Thread(target=reader)
    t.start()

    # Safety watchdog to prevent hanging tests
    def watchdog():
        time.sleep(float(os.environ.get('RETOLD_PTY_SECONDS', '12')))
        try:
            os.kill(pid, signal.SIGKILL)
        except OSError:
            pass
    w = threading.Thread(target=watchdog, daemon=True)
    w.start()

    # Wait until ratatui has entered alternate screen (terminal is in raw mode and ready)
    for _ in range(60):
        if any(b'\x1b[?1049h' in c for c in chunks):
            break
        time.sleep(0.05)
    time.sleep(0.15)

    output_start = 0

    def ready(action):
        if action.startswith('WAIT_FILE:'):
            path = action[len('WAIT_FILE:'):]
            return os.path.isfile(path) and os.path.getsize(path) > 0
        needle = action[len('WAIT:'):].encode('utf-8')
        # Join chunks because a terminal write may be split across reads.
        return needle in b''.join(chunks)[output_start:]

    for action in actions:
        if action.startswith(('WAIT:', 'WAIT_FILE:')):
            deadline = time.monotonic() + 5.0
            while not ready(action) and time.monotonic() < deadline:
                time.sleep(0.02)
            if not ready(action):
                sys.stderr.write('PTY readiness timed out: ' + action + '\n')
                try:
                    os.kill(pid, signal.SIGKILL)
                except OSError:
                    pass
                break
            continue
        if action.startswith('EXTERNAL_SWITCH:'):
            target = action[len('EXTERNAL_SWITCH:'):]
            external_env = os.environ.copy()
            external_env['RETOLD_CONFIG_DIR'] = config
            subprocess.run([bin_path, '--json', 'workspace', 'switch', target],
                           env=external_env, capture_output=True, check=True, timeout=5)
        elif action.startswith('COPY_FILE:'):
            source, target = action[len('COPY_FILE:'):].split('|', 1)
            shutil.copyfile(source, target)
        elif action.startswith('RESIZE:'):
            width, height = map(int, action[len('RESIZE:'):].split('x'))
            output_start = len(b''.join(chunks))
            fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack('HHHH', height, width, 0, 0))
            os.kill(pid, signal.SIGWINCH)
        elif action == 'ASSERT_RAW':
            attributes = termios.tcgetattr(slave)
            if attributes[3] & (termios.ECHO | termios.ICANON):
                sys.stderr.write('PTY raw input was lost while the app was active.\n')
                os.kill(pid, signal.SIGKILL)
                break
        elif action.startswith('SIGNAL:'):
            os.kill(pid, getattr(signal, action[len('SIGNAL:'):]))
        elif action.startswith('SLEEP:'):
            time.sleep(float(action[6:]))
        elif action.startswith('KEY:'):
            k = action[4:].encode('utf-8')
            try:
                output_start = len(b''.join(chunks))
                os.write(master, k)
            except OSError:
                break

    _, status = os.waitpid(pid, 0)
    restored_attributes = termios.tcgetattr(slave)
    os.close(slave)
    t.join(2.0)
    raw = b''.join(chunks)
    sys.stdout.buffer.write(raw)
    exit_code = os.waitstatus_to_exitcode(status)
    # PENDIN describes kernel input-buffer state, not a mode setting: restoring canonical
    # input sets it until the next read on macOS, even when the original settings match.
    original_modes = original_attributes.copy()
    restored_modes = restored_attributes.copy()
    for attributes in (original_modes, restored_modes):
        attributes[3] &= ~getattr(termios, 'PENDIN', 0)
    if restored_modes != original_modes:
        sys.stderr.write('PTY terminal attributes were not restored.\n')
        changed = [index for index, pair in enumerate(zip(original_attributes, restored_attributes)) if pair[0] != pair[1]]
        sys.stderr.write('Changed attribute indexes: ' + repr(changed) + '\n')
        sys.stderr.write('Original flags: ' + repr(original_attributes[:4]) + '\n')
        sys.stderr.write('Restored flags: ' + repr(restored_attributes[:4]) + '\n')
        if exit_code == 0:
            exit_code = 89
    sys.exit(exit_code)
