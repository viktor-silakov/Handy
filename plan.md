# Remote Transcription Server Implementation Plan

- [x] **Repository:** The project fork must be located at `git@github.com:viktor-silakov/Handy.git`.

## Part 1: Client Application (Handy)

Goal: Allow users to use external (remote) transcription servers (GigaAM, Whisper Turbo, etc.) instead of local execution.

### 1.1 UI and Settings

- [x] Add a new engine choice to the application settings: "Remote Server".
- [x] Add text fields for remote server configuration:
  - [x] **Server URL** (e.g., `http://localhost:3000` or a remote address).
  - [x] **API Token** for authorization.
- [x] Update Zustand/Tauri store for secure persistence.

### 1.2 Backend/Tauri

- [x] Update types (`EngineType`) in `src/bindings.ts` and corresponding Rust enums (`src-tauri/src/managers/transcription.rs`, `src-tauri/src/managers/model.rs`), adding a type for the remote engine (e.g., `RemoteEngine`).
- [x] Implement logic to send audio data to the specified server URL instead of calling local libraries.
- [x] Add API token transmission in request headers (e.g., `Authorization: Bearer <API-Token>`).
- [x] Implement handling for network errors and invalid tokens with clear user messages.

## Part 2: CLI and Server Package for `npx`

Goal: Create a separate npm package that allows spinning up an inference server with a single terminal command.

### 2.1 Package Initialization

- [x] Create a new project/package to be published to npm (`handy-remote-server`).
- [x] Configure `bin` in `package.json` for CLI execution.
- [x] Select a lightweight web framework (Express) for handling HTTP requests.

### 2.2 Model and Inference Management

- [x] Implement model loading similar to the desktop app.
- [x] **Default Model:** GigaAM.
- [x] **Automatic Download:** The server should check for model weights locally on startup. If missing, automatically download them.
- [x] Implement command-line parameter support for starting specific models (e.g., `npx <package> --model whisper-turbo`).

### 2.3 Authentication

- [x] Implement middleware for checking the `API Key` on every inference request.
- [x] Check for an environment variable (e.g., `API_KEY`) at server startup.
- [x] If the environment variable is not set, automatically generate a secure random token.
- [x] Output the generated (or provided) token to the terminal on success: `Server started. Your API KEY is: <token>`.

### 2.4 Routing and API

- [x] Create endpoints compatible with the desktop client's expected format (e.g., `POST /transcribe` with audio file transmission).
- [x] Link endpoints to model execution logic.

### 2.5 Testing, Deployment, and Publication

- [x] Ensure the server starts correctly via `npx <package-name>`.
- [x] Verify the flow: start server -> auto-download -> key generation -> successful connection from Desktop client -> successful transcription.
- [x] **Required:** Performance testing (local) and optimization.
- [x] **Required:** Publish the npm package.

## Part 3: Documentation

- [x] **Required:** Update project documentation, describing remote server usage and npx package execution.
