import type { PluginAPI } from '@ampcode/plugin'
import { execFile } from 'node:child_process'
import { createHash } from 'node:crypto'
import { realpath, stat } from 'node:fs/promises'
import { basename, dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { promisify } from 'node:util'

export const description = 'Open local Markdown in a live, scrollable Rex pane without leaving Amp.'

const exec = promisify(execFile)
const viewer = fileURLToPath(new URL('./viewer.py', import.meta.url))

async function rex(...args: string[]) {
	const { stdout } = await exec('rex', ['--autostart=false', ...args], { timeout: 15_000 })
	return JSON.parse(stdout)
}

export default function (amp: PluginAPI) {
	// Serialize opens so concurrent calls cannot both create the same preview.
	let pending: Promise<unknown> = Promise.resolve()
	amp.registerTool({
		name: 'rex_open_markdown',
		title: 'Open live Markdown in Rex',
		description: 'Open a local Markdown file in a read-only, live-refreshing Rex split beside Amp. Reuses the preview for this thread and file. Does not take focus. Requires Rex and uv on this executor; it cannot open a pane on another machine. Edit the original file normally to update the preview. The user can scroll, press t for contents, and q to close. Use for documents, design whiteboards, and plans the user wants to see in Rex.',
		inputSchema: {
			type: 'object',
			properties: {
				path: { type: 'string', description: 'Markdown file, absolute or relative to the workspace root.' },
				anchor: { type: 'string', description: 'Optional full Rex block ID to split beside; defaults to the inherited Amp pane. Required outside Rex.' },
			},
			required: ['path'],
			additionalProperties: false,
		},
		execute(input, ctx) {
			const open = async () => {
				if (typeof input.path !== 'string' || !input.path.trim()) throw new Error('A Markdown path is required.')
				const root = amp.system.workspaceRoot
					? amp.helpers.filePathFromURI(amp.system.workspaceRoot) : process.cwd()
				const path = await realpath(resolve(root, input.path))
				if (!(await stat(path)).isFile()) throw new Error('The Markdown path must be a regular file.')
				const identity = await rex('whoami', '--json')
				if (!identity.endpoint?.startsWith('unix:')) {
					throw new Error('This preview requires a local Rex server (Unix socket); files are not transferred to remote servers.')
				}
				const anchor = input.anchor ?? process.env.REX_BLOCK
				if (typeof anchor !== 'string' || !anchor.startsWith('block:')) {
					throw new Error('Run Amp inside Rex, or provide the full anchor block ID. No pane was opened.')
				}
				const target = await rex('block', 'inspect', anchor, '--json')
				const session = target.session_id
				const key = createHash('sha256').update(JSON.stringify([ctx.thread.id, path])).digest('hex').slice(0, 16)
				const label = `amp-md-${key}: ${basename(path)}`
				const { blocks } = await rex('block', 'ls', '-s', session, '--json')
				for (const block of blocks) {
					if (block.label !== label || block.window_id !== target.block.window_id) continue
					const state = await rex('block', 'call', '-s', session, '-b', block.block_id, 'process')
					if (state.child && !state.last_exit) {
						return JSON.stringify({ reused: true, path, session, block: block.block_id })
					}
				}
				// Resolve dependencies before creating a pane, so install failures do not leave broken splits.
				const { stdout } = await exec('uv', ['run', '--script', viewer, '--check'], { timeout: 120_000 })
				const python = stdout.trim()
				const created = await rex('split', '-s', session, '-b', anchor,
					'--split=right', '--ratio=50', '--focus=false', '--shell=none',
					'--label', label, '--cwd', dirname(path), '--json', '--', python, viewer, path)
				return JSON.stringify({ reused: false, path, session, block: created.block_ids[0],
					note: 'Preview launched. File changes refresh automatically. Use rex capture to inspect startup/rendering.' })
			}
			const result = pending.then(open)
			pending = result.catch(() => {})
			return result
		},
	})
}
