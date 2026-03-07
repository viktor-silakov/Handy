# Handy Remote Server 🎙️

A lightweight standalone inference server for [Handy](https://github.com/cjpais/Handy), allowing you to transcribe audio from external devices, weak computers, and more.

## Installation

The easiest way to run the external inference server is using `npx`:

```bash
npx handy-remote-server
```

_(You must have Node.js and npm installed)_

## Usage

When you run the server for the first time, it will automatically download the **GigaAM v3** model (Russian-only fast architecture model) if it's not present.

It will also generate a unique Bearer API Token for your active session:

```bash
Your API KEY is: xxxxx-xxxxx-xxxxx-xxxxx
Handy Remote Server is running on port 3000
```

1. Open **Handy** on your client machine.
2. Go to **Settings > General**, select the `Remote` engine.
3. Provide the Server URL: `http://<your-server-ip>:3000`
4. Provide the generated Token.
5. All audio chunks will now be transcribed by the server!

## How it works

The `handy-remote-server` spins up a tiny Express server alongside a heavily optimized Rust CLI (`rust-infer`) powered by `transcribe-rs`. Audio files are dispatched sequentially from the Node server directly into the Rust engine.

### Environment variables

- `PORT` - defaults to `3000`
- `API_KEY` - defaults to an auto-generated token in development. Set this to a permanent token for production.
