// Starts `stylet lsp` for stylet files and restarts it when stylet.toml changes.
const vscode = require('vscode')
const { LanguageClient } = require('vscode-languageclient/node')

let client

function start() {
  const folder = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath
  const command = vscode.workspace.getConfiguration('stylet').get('path', 'stylet')
  client = new LanguageClient(
    'stylet',
    'stylet',
    { command, args: ['lsp'], options: { cwd: folder } },
    {
      documentSelector: [{ scheme: 'file', language: 'stylet' }],
      synchronize: { configurationSection: 'stylet' },
    },
  )
  return client.start().catch((error) => {
    vscode.window.showErrorMessage(
      `Couldn't start the stylet language server ("${command} lsp"): ${error.message}. ` +
        'Install stylet or set "stylet.path".',
    )
  })
}

async function restart() {
  if (client) await client.stop().catch(() => {})
  await start()
}

function activate(context) {
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
  )
  return start()
}

function deactivate() {
  return client?.stop()
}

module.exports = { activate, deactivate }
