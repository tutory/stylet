// Starts `stylet lsp` for stylet files and restarts it when stylet.toml changes.
const fs = require('fs')
const path = require('path')
const vscode = require('vscode')
const { LanguageClient } = require('vscode-languageclient/node')

const EXE = process.platform === 'win32' ? 'stylet.exe' : 'stylet'

let client
let extensionPath

/**
 * The stylet binary: the `stylet.path` setting, the workspace's
 * `@tutory_de/stylet` npm package (in trusted workspaces), the binary bundled
 * with the extension, or `stylet` on PATH.
 */
function serverCommand(folder) {
  const configured = vscode.workspace.getConfiguration('stylet').get('path')
  if (configured) return { command: configured, source: 'the "stylet.path" setting' }
  if (folder && vscode.workspace.isTrusted) {
    const pkg = `stylet-${process.platform}-${process.arch}`
    const local = path.join(folder, 'node_modules', '@tutory_de', pkg, 'bin', EXE)
    if (fs.existsSync(local)) return { command: local, source: 'the workspace\'s @tutory_de/stylet' }
  }
  const bundled = path.join(extensionPath, 'bin', EXE)
  if (fs.existsSync(bundled)) return { command: bundled, source: 'the extension' }
  return { command: 'stylet', source: 'PATH' }
}

function start() {
  const folder = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath
  const { command, source } = serverCommand(folder)
  client = new LanguageClient(
    'stylet',
    'stylet',
    { command, args: ['lsp'], options: { cwd: folder } },
    {
      documentSelector: [{ scheme: 'file', language: 'stylet' }],
      synchronize: { configurationSection: 'stylet' },
    },
  )
  client.outputChannel.appendLine(`Using ${command} (from ${source})`)
  return client.start().catch((error) => {
    vscode.window.showErrorMessage(
      `Couldn't start the stylet language server ("${command} lsp", from ${source}): ${error.message}. ` +
        'Install @tutory_de/stylet in the project or set "stylet.path".',
    )
  })
}

async function restart() {
  if (client) await client.stop().catch(() => {})
  await start()
}

function activate(context) {
  extensionPath = context.extensionPath
  const watcher = vscode.workspace.createFileSystemWatcher('**/stylet.toml')
  watcher.onDidChange(restart)
  watcher.onDidCreate(restart)
  watcher.onDidDelete(restart)
  context.subscriptions.push(
    watcher,
    vscode.commands.registerCommand('stylet.restartServer', restart),
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration('stylet.path')) restart()
    }),
    vscode.workspace.onDidGrantWorkspaceTrust(restart),
  )
  return start()
}

function deactivate() {
  return client?.stop()
}

module.exports = { activate, deactivate }
