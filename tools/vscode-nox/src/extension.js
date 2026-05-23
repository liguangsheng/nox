const vscode = require('vscode');
const { LanguageClient, TransportKind } = require('vscode-languageclient/node');

let client;

function resolveNoxBinary() {
  const configured = vscode.workspace.getConfiguration('nox').get('binaryPath');
  if (configured && configured.trim() !== '') {
    return configured;
  }
  if (process.env.NOX_BINARY && process.env.NOX_BINARY.trim() !== '') {
    return process.env.NOX_BINARY;
  }
  return 'nox';
}

function activate(context) {
  const command = resolveNoxBinary();
  client = new LanguageClient(
    'nox',
    'Nox Language Server',
    {
      command,
      args: ['lsp'],
      transport: TransportKind.stdio
    },
    {
      documentSelector: [{ scheme: 'file', language: 'nox' }],
      synchronize: {
        fileEvents: vscode.workspace.createFileSystemWatcher('**/*.nox')
      }
    }
  );
  context.subscriptions.push(client.start());
  context.subscriptions.push(
    vscode.debug.registerDebugAdapterDescriptorFactory('nox', {
      createDebugAdapterDescriptor() {
        return new vscode.DebugAdapterExecutable(command, ['dap']);
      }
    })
  );
}

function deactivate() {
  if (!client) {
    return undefined;
  }
  return client.stop();
}

module.exports = { activate, deactivate, resolveNoxBinary };
