# Handy Remote Server 🎙️

A lightweight standalone inference server for [Handy](https://github.com/viktor-silakov/Handy), allowing you to transcribe audio from external devices, weak computers, and more.

## Installation

The easiest way to run the external inference server is using `npx`:

```bash
npx handy-remote-server
```

_(You must have Node.js and Rust/Cargo installed)_

## Usage

When you run the server for the first time, it will:

1. **Download the GigaAM v3 model** (~100 MB) with a progress bar:

```
📥 Downloading model...
   URL:  https://blob.handy.computer/giga-am-v3.int8.onnx
   Dest: /path/to/models/gigaam.onnx

   ████████████████████░░░░░░░░░░░░░░░░░░░░  52.3%  52.10 MB / 99.60 MB  12.5 MB/s

✅ Download complete in 8.2s
```

2. **Generate a persistent API key** saved to `~/.handy/api_key`:

```
======================================================
Generated a new API KEY (saved to /Users/you/.handy/api_key)
Your API KEY is: xxxxx...xxxxx
======================================================
```

The key persists across restarts. On the next launch, it will be loaded automatically.

3. **Start the server** and log every request in detail:

```
Handy Remote Server is running on port 3000

[2026-03-07T12:00:00.000Z] ── REQUEST #1 ──────────────────────
  Method:  POST /transcribe
  From:    192.168.1.5
  Auth:    OK
  [#1]  Audio received: 156.3 KB
  [#1]  Queued for inference (queue length: 0)
[2026-03-07T12:00:01.234Z] ── RESPONSE #1 ─────────────────────
  Status:   200
  Duration: 1.23s
  Result:   "Hello, how are you?"
```

### Connecting from Handy

1. Open **Handy** on your client machine.
2. Go to **Settings > Models**, select **Remote Server**.
3. Go to **Settings > General**, fill in:
   - **Remote Server URL**: `http://<your-server-ip>:3000`
   - **API Token**: the generated token
4. All transcriptions will now be processed by the server!

## Environment Variables

| Variable         | Default                                     | Description                            |
| ---------------- | ------------------------------------------- | -------------------------------------- |
| `PORT`           | `3000`                                      | Server port                            |
| `API_KEY`        | auto-generated, saved to `~/.handy/api_key` | Bearer token for authentication        |
| `INFER_CLI_PATH` | auto-detected                               | Path to the `rust-infer` binary        |
| `MODEL_TYPE`     | `gigaam`                                    | Transcription model to use (see below) |

## Supported Models

```bash
# Russian (default)
MODEL_TYPE=gigaam npx handy-remote-server

# Multi-language (including Russian) — Whisper models
MODEL_TYPE=whisper-tiny npx handy-remote-server      # 75 MB
MODEL_TYPE=whisper-base npx handy-remote-server      # 142 MB
MODEL_TYPE=whisper-small npx handy-remote-server     # 487 MB
MODEL_TYPE=whisper-medium npx handy-remote-server    # 1.5 GB
MODEL_TYPE=whisper-turbo npx handy-remote-server     # 1.6 GB
MODEL_TYPE=whisper-large npx handy-remote-server     # 1.1 GB

# English — Moonshine
MODEL_TYPE=moonshine-tiny npx handy-remote-server    # 60 MB
MODEL_TYPE=moonshine-base npx handy-remote-server    # 100 MB

# English — Breeze/Parakeet
MODEL_TYPE=parakeet npx handy-remote-server          # ~200 MB

# Multi-language — SenseVoice
MODEL_TYPE=sensevoice npx handy-remote-server        # ~200 MB
```

| Model              | Language       | Size    | Speed     |
| ------------------ | -------------- | ------- | --------- |
| `gigaam` (default) | Russian        | ~100 MB | ⚡ Fast   |
| `whisper-tiny`     | Multi-language | 75 MB   | ⚡ Fast   |
| `whisper-base`     | Multi-language | 142 MB  | ⚡ Fast   |
| `whisper-small`    | Multi-language | 487 MB  | 🔄 Medium |
| `whisper-medium`   | Multi-language | 1.5 GB  | 🐢 Slow   |
| `whisper-turbo`    | Multi-language | 1.6 GB  | ⚡ Fast   |
| `whisper-large`    | Multi-language | 1.1 GB  | 🐢 Slow   |
| `moonshine-tiny`   | English        | 60 MB   | ⚡ Fast   |
| `moonshine-base`   | English        | 100 MB  | ⚡ Fast   |
| `parakeet`         | English        | ~200 MB | 🔄 Medium |
| `sensevoice`       | Multi-language | ~200 MB | 🔄 Medium |

## How It Works

The `handy-remote-server` spins up a tiny Express server alongside a heavily optimized Rust CLI (`rust-infer`) powered by `transcribe-rs`. Audio files are dispatched sequentially from the Node server directly into the Rust engine.

Currently the server uses the **GigaAM v3** model (Russian-language, fast inference, ~100 MB).
