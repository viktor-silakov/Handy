#!/usr/bin/env node

import express from 'express';
import multer from 'multer';
import { spawn } from 'child_process';
import crypto from 'crypto';
import path from 'path';
import fs from 'fs';
import os from 'os';
import dotenv from 'dotenv';
import tar from 'tar-fs';
import gunzip from 'gunzip-maybe';

dotenv.config();

const app = express();
const port = process.env.PORT || 3000;

// ── Persistent API Key ────────────────────────────────────────────────
const handyDir = path.join(os.homedir(), '.handy');
const keyFilePath = path.join(handyDir, 'api_key');

function loadOrCreateApiKey(): string {
    if (process.env.API_KEY) return process.env.API_KEY;
    if (fs.existsSync(keyFilePath)) {
        const cached = fs.readFileSync(keyFilePath, 'utf-8').trim();
        if (cached.length > 0) return cached;
    }
    const newKey = crypto.randomBytes(32).toString('hex');
    fs.mkdirSync(handyDir, { recursive: true });
    fs.writeFileSync(keyFilePath, newKey + '\n', { mode: 0o600 });
    return newKey;
}

const API_KEY = loadOrCreateApiKey();

// ── Logging helpers ───────────────────────────────────────────────────
function timestamp(): string { return new Date().toISOString(); }
function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}
function formatDuration(ms: number): string {
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(2)}s`;
}

// ── Registry of supported models ──────────────────────────────────────
interface ModelConfig {
    engine: string;
    url: string;
    filename: string;
    isArchive?: boolean;
    configFilename?: string; // For parakeet (preprocessor.json)
}

const MODEL_REGISTRY: Record<string, ModelConfig> = {
    'gigaam': {
        engine: 'gigaam',
        url: 'https://blob.handy.computer/giga-am-v3.int8.onnx',
        filename: 'gigaam.onnx'
    },
    'whisper-tiny': {
        engine: 'whisper',
        url: 'https://blob.handy.computer/ggml-tiny.bin',
        filename: 'whisper-tiny.bin'
    },
    'whisper-base': {
        engine: 'whisper',
        url: 'https://blob.handy.computer/ggml-base.bin',
        filename: 'whisper-base.bin'
    },
    'whisper-small': {
        engine: 'whisper',
        url: 'https://blob.handy.computer/ggml-small.bin',
        filename: 'whisper-small.bin'
    },
    'whisper-medium': {
        engine: 'whisper',
        url: 'https://blob.handy.computer/whisper-medium-q4_1.bin',
        filename: 'whisper-medium.bin'
    },
    'whisper-turbo': {
        engine: 'whisper',
        url: 'https://blob.handy.computer/ggml-large-v3-turbo.bin',
        filename: 'whisper-turbo.bin'
    },
    'whisper-large': {
        engine: 'whisper',
        url: 'https://blob.handy.computer/ggml-large-v3-q5_0.bin',
        filename: 'whisper-large.bin'
    },
    'moonshine-tiny': {
        engine: 'moonshine',
        url: 'https://blob.handy.computer/moonshine-tiny-streaming-en.tar.gz',
        filename: 'moonshine-tiny', // Dir name after extraction
        isArchive: true
    },
    'moonshine-base': {
        engine: 'moonshine',
        url: 'https://blob.handy.computer/moonshine-base.tar.gz',
        filename: 'moonshine-base',
        isArchive: true
    },
    'parakeet': {
        engine: 'parakeet',
        url: 'https://blob.handy.computer/parakeet-v3-int8.tar.gz',
        filename: 'parakeet-v3',
        isArchive: true,
        configFilename: 'preprocessor.json'
    },
    'sensevoice': {
        engine: 'sensevoice',
        url: 'https://blob.handy.computer/sense-voice-int8.tar.gz',
        filename: 'sensevoice',
        isArchive: true
    }
};

const SELECTED_MODEL_TYPE = (process.env.MODEL_TYPE || 'gigaam').toLowerCase();
const modelCfg = MODEL_REGISTRY[SELECTED_MODEL_TYPE];

if (!modelCfg) {
    console.error(`Error: Unknown MODEL_TYPE "${SELECTED_MODEL_TYPE}".`);
    console.error(`Supported types: ${Object.keys(MODEL_REGISTRY).join(', ')}`);
    process.exit(1);
}

// ── Directories ───────────────────────────────────────────────────────
const modelsBaseDir = path.join(__dirname, '..', 'models');
const uploadDir = path.join(__dirname, '..', 'uploads');
[modelsBaseDir, uploadDir].forEach(d => { if (!fs.existsSync(d)) fs.mkdirSync(d, { recursive: true }); });

// ── Model paths ───────────────────────────────────────────────────────
const modelPath = path.join(modelsBaseDir, modelCfg.filename);
let actualModelFile = modelPath;
let parakeetConfigPath = '';

if (modelCfg.isArchive) {
    // For archives, we look for model.onnx inside the directory
    actualModelFile = path.join(modelPath, 'model.onnx');
    if (modelCfg.engine === 'parakeet') {
        parakeetConfigPath = path.join(modelPath, modelCfg.configFilename!);
    }
}

// ── Download & Extract ────────────────────────────────────────────────
async function downloadAndPrepare() {
    if (fs.existsSync(actualModelFile)) return;

    const dest = modelCfg.isArchive ? modelPath + '.tar.gz' : modelPath;
    console.log(`\n📥 Downloading model: ${SELECTED_MODEL_TYPE}...`);
    console.log(`   URL:  ${modelCfg.url}`);

    const response = await fetch(modelCfg.url);
    if (!response.ok) throw new Error(`Failed to fetch: ${response.statusText}`);

    const totalBytes = parseInt(response.headers.get('content-length') || '0', 10);
    let downloadedBytes = 0;
    const startTime = Date.now();
    const fileStream = fs.createWriteStream(dest);
    const reader = response.body?.getReader();
    if (!reader) throw new Error('Body not readable');

    const barWidth = 40;
    while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        fileStream.write(Buffer.from(value));
        downloadedBytes += value.length;
        const percent = totalBytes > 0 ? downloadedBytes / totalBytes : 0;
        const filled = Math.round(barWidth * percent);
        const bar = '█'.repeat(filled) + '░'.repeat(barWidth - filled);
        const pct = (percent * 100).toFixed(1).padStart(5);
        const speed = (downloadedBytes / ((Date.now() - startTime) / 1000) / 1024 / 1024).toFixed(1);
        process.stdout.write(`\r   ${bar} ${pct}%  ${formatBytes(downloadedBytes)} / ${formatBytes(totalBytes)}  ${speed} MB/s   `);
    }
    await new Promise<void>(r => fileStream.end(() => r()));
    process.stdout.write('\n');

    if (modelCfg.isArchive) {
        console.log(`📦 Extracting archive to ${modelPath}...`);
        fs.mkdirSync(modelPath, { recursive: true });
        await new Promise((resolve, reject) => {
            fs.createReadStream(dest)
                .pipe(gunzip())
                .pipe(tar.extract(modelPath))
                .on('finish', resolve)
                .on('error', reject);
        });
        fs.unlinkSync(dest); // Cleanup
    }
    console.log(`✅ Ready!\n`);
}

// ── Request logging ───────────────────────────────────────────────────
let requestCounter = 0;
app.use((req, res, next) => {
    const reqId = ++requestCounter;
    const start = Date.now();
    const ip = req.headers['x-forwarded-for'] || req.socket.remoteAddress || 'unknown';
    console.log(`\n[${timestamp()}] ── REQUEST #${reqId} ──────────────────────`);
    console.log(`  Method:  ${req.method} ${req.path}`);
    console.log(`  Model:   ${SELECTED_MODEL_TYPE}`);
    (req as any)._reqId = reqId;
    (req as any)._startTime = start;
    const originalJson = res.json.bind(res);
    res.json = function (body: any) {
        console.log(`[${timestamp()}] ── RESPONSE #${reqId} (Status: ${res.statusCode}, ${Date.now() - start}ms) ─────`);
        if (body?.text) console.log(`  Result:   "${body.text.substring(0, 100)}${body.text.length > 100 ? '...' : ''}"`);
        else if (body?.error) console.log(`  Error:    ${body.error}`);
        return originalJson(body);
    };
    next();
});

// ── Auth ──────────────────────────────────────────────────────────────
app.use((req, res, next) => {
    const authHeader = req.headers.authorization;
    if (!authHeader?.startsWith('Bearer ') || authHeader.split(' ')[1] !== API_KEY) {
        return res.status(401).json({ error: 'Auth failed' });
    }
    next();
});

// ── Inference Bridge ──────────────────────────────────────────────────
let inferProcess: any = null;
let isReady = false;
let resolvers: Record<string, Function> = {};

downloadAndPrepare().then(() => {
    let binPath = process.env.INFER_CLI_PATH || path.join(__dirname, '..', 'rust-infer', 'target', 'release', 'rust-infer');
    if (!fs.existsSync(binPath)) binPath = path.join(__dirname, '..', 'rust-infer', 'target', 'debug', 'rust-infer');

    console.log(`Starting inference: ${binPath}`);
    const args = [modelCfg.engine, actualModelFile];
    if (parakeetConfigPath) args.push(parakeetConfigPath);

    inferProcess = spawn(binPath, args, { stdio: ['pipe', 'pipe', 'inherit'] });
    inferProcess.stdout.on('data', (data: Buffer) => {
        data.toString().split('\n').filter(Boolean).forEach(line => {
            if (line.trim() === 'READY') {
                isReady = true;
                console.log('--- Model fully loaded and ready ---');
                return;
            }
            try {
                const parsed = JSON.parse(line);
                const firstKey = Object.keys(resolvers)[0];
                if (firstKey) {
                    resolvers[firstKey](parsed);
                    delete resolvers[firstKey];
                }
            } catch { }
        });
    });
    inferProcess.on('exit', (code: number) => process.exit(code || 1));
}).catch(e => { console.error(e); process.exit(1); });

const requestQueue: { file: string, resolve: Function, reqId: number }[] = [];
let isProcessing = false;

function processQueue() {
    if (isProcessing || requestQueue.length === 0 || !isReady) return;
    isProcessing = true;
    const req = requestQueue.shift()!;
    resolvers[req.file] = (result: any) => {
        isProcessing = false;
        if (fs.existsSync(req.file)) fs.unlinkSync(req.file);
        req.resolve(result);
        processQueue();
    };
    inferProcess.stdin.write(req.file + '\n');
}

app.post('/transcribe', express.raw({ type: 'audio/wav', limit: '100mb' }), async (req, res) => {
    if (!isReady) return res.status(503).json({ error: 'Starting up' });
    const tempFile = path.join(uploadDir, `up-${Date.now()}.wav`);
    fs.writeFileSync(tempFile, req.body);
    const result = await new Promise(r => {
        requestQueue.push({ file: tempFile, resolve: r, reqId: (req as any)._reqId });
        processQueue();
    });
    res.json(result);
});

app.listen(port, () => console.log(`\nHandy Server on port ${port} | API Key: ${API_KEY}`));
