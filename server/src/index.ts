#!/usr/bin/env node

import express from 'express';
import multer from 'multer';
import { spawn } from 'child_process';
import crypto from 'crypto';
import path from 'path';
import fs from 'fs';
import dotenv from 'dotenv';
import { Transform } from 'stream';

dotenv.config();

const app = express();
const port = process.env.PORT || 3000;

// Set up API KEY
let API_KEY = process.env.API_KEY;
if (!API_KEY) {
    API_KEY = crypto.randomBytes(32).toString('hex');
    console.log(`\n======================================================`);
    console.log(`Server started without API_KEY in environment variables.`);
    console.log(`Generated a new token for this session.`);
    console.log(`Your API KEY is: ${API_KEY}`);
    console.log(`======================================================\n`);
}

// Ensure models directory exists
const modelsDir = path.join(__dirname, '..', 'models');
if (!fs.existsSync(modelsDir)) {
    fs.mkdirSync(modelsDir, { recursive: true });
}

// Authentication middleware
app.use((req, res, next) => {
    const authHeader = req.headers.authorization;
    if (!authHeader || !authHeader.startsWith('Bearer ')) {
        return res.status(401).json({ error: 'Missing or invalid Authorization header' });
    }

    const token = authHeader.split(' ')[1];
    if (token !== API_KEY) {
        return res.status(403).json({ error: 'Invalid API Key' });
    }

    next();
});

// Configure multer to store uploaded files in a temp directory
const uploadDir = path.join(__dirname, '..', 'uploads');
if (!fs.existsSync(uploadDir)) {
    fs.mkdirSync(uploadDir, { recursive: true });
}

const storage = multer.diskStorage({
    destination: function (req, file, cb) {
        cb(null, uploadDir);
    },
    filename: function (req, file, cb) {
        const uniqueSuffix = Date.now() + '-' + Math.round(Math.random() * 1E9);
        cb(null, file.fieldname + '-' + uniqueSuffix + '.wav');
    }
});

const upload = multer({ storage: storage });

// Model download logic
const GIGAAM_MODEL_URL = 'https://blob.handy.computer/giga-am-v3.int8.onnx';
const gigaamModelPath = path.join(modelsDir, 'gigaam.onnx');

async function downloadFile(url: string, dest: string) {
    if (fs.existsSync(dest)) return;
    console.log(`Downloading ${url} to ${dest}...`);
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    const response = await fetch(url);
    if (!response.ok) throw new Error(`Failed to fetch ${url}: ${response.statusText}`);
    const arrBuffer = await response.arrayBuffer();
    fs.writeFileSync(dest, Buffer.from(arrBuffer));
    console.log(`Downloaded ${dest}`);
}

async function ensureModels() {
    await downloadFile(GIGAAM_MODEL_URL, gigaamModelPath);
}

let inferProcess: any = null;
let isReady = false;
let resolvers: Record<string, Function> = {};

ensureModels().then(() => {
    // Spawn the rust background process
    let inferProcessPath = process.env.INFER_CLI_PATH || path.join(__dirname, '..', 'rust-infer', 'target', 'release', 'rust-infer');
    if (!fs.existsSync(inferProcessPath)) {
        inferProcessPath = path.join(__dirname, '..', 'rust-infer', 'target', 'debug', 'rust-infer');
    }

    console.log(`Using inference CLI: ${inferProcessPath}`);

    inferProcess = spawn(inferProcessPath, [gigaamModelPath], { stdio: ['pipe', 'pipe', 'inherit'] });

    inferProcess.stdout.on('data', (data: Buffer) => {
        const lines = data.toString().split('\n').map((l: string) => l.trim()).filter(Boolean);

        for (const line of lines) {
            if (line === 'READY') {
                isReady = true;
                console.log('Inference worker is ready.');
                continue;
            }

            try {
                const parsed = JSON.parse(line);
                const resolverCount = Object.keys(resolvers).length;
                if (resolverCount > 0) {
                    const firstKey = Object.keys(resolvers)[0];
                    resolvers[firstKey](parsed);
                }
            } catch (e) {
                console.log('Got non-JSON output from worker:', line);
            }
        }
    });

    inferProcess.on('exit', (code: number) => {
        console.log(`Inference worker exited with code ${code}`);
        process.exit(code || 1);
    });
}).catch(e => {
    console.error('Failed to download models:', e);
    process.exit(1);
});

// Queue for pending transcriptions to send them sequentially to the single worker
const requestQueue: { file: string, resolve: Function }[] = [];
let isProcessing = false;

function processQueue() {
    if (isProcessing || requestQueue.length === 0 || !isReady) return;

    isProcessing = true;
    const req = requestQueue.shift()!;

    // Register resolver
    resolvers[req.file] = (result: any) => {
        delete resolvers[req.file];
        isProcessing = false;
        // Clean up temp file
        if (fs.existsSync(req.file)) {
            fs.unlinkSync(req.file);
        }
        req.resolve(result);
        // Process next
        process.nextTick(processQueue);
    };

    // Send path to worker via stdin
    inferProcess.stdin.write(req.file + '\n');
}

// The route
app.post('/transcribe', express.raw({ type: 'audio/wav', limit: '50mb' }), async (req, res) => {
    if (!isReady) {
        return res.status(503).json({ error: 'Models are still loading' });
    }

    // If using express.raw, the body is a Buffer
    if (!req.body || !Buffer.isBuffer(req.body)) {
        return res.status(400).json({ error: 'Invalid audio body. Send raw WAV bytes with Content-Type: audio/wav' });
    }

    const tempFilePath = path.join(uploadDir, `upload-${Date.now()}-${Math.random().toString(36).substring(7)}.wav`);
    fs.writeFileSync(tempFilePath, req.body);

    const result = await new Promise((resolve) => {
        requestQueue.push({ file: tempFilePath, resolve });
        processQueue();
    });

    res.json(result);
});

app.listen(port, () => {
    console.log(`Handy Remote Server is running on port ${port}`);
});
