import { spawn } from 'node:child_process'
import { existsSync } from 'node:fs'
import { homedir } from 'node:os'
import { delimiter, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createRequire } from 'node:module'

// Rustup may be installed without changing the user's global PATH.
const env = { ...process.env }
const cargoBin = join(env.CARGO_HOME || join(homedir(), '.cargo'), 'bin')
const pathKey = Object.keys(env).find(key => key.toLowerCase() === 'path') || 'PATH'
if (existsSync(cargoBin)) env[pathKey] = cargoBin + delimiter + (env[pathKey] || '')
const require = createRequire(import.meta.url)
const cli = require.resolve('@tauri-apps/cli/tauri.js')
const child = spawn(process.execPath, [cli, ...process.argv.slice(2)], {
  cwd: fileURLToPath(new URL('..', import.meta.url)),
  env,
  stdio: 'inherit',
})
child.on('error', error => { console.error('无法启动桌面程序：', error.message); process.exitCode = 1 })
child.on('exit', code => { process.exitCode = code ?? 1 })
