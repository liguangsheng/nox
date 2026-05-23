const fs = require('fs');
const path = require('path');

const root = path.resolve(__dirname, '..');
for (const file of [
  'package.json',
  'src/extension.js',
  'syntaxes/nox.tmLanguage.json',
  'language-configuration/nox.json'
]) {
  const full = path.join(root, file);
  if (!fs.existsSync(full)) {
    throw new Error(`missing ${file}`);
  }
}

const manifest = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
if (!manifest.contributes.languages.some((language) => language.id === 'nox')) {
  throw new Error('missing nox language contribution');
}
if (!manifest.contributes.grammars.some((grammar) => grammar.scopeName === 'source.nox')) {
  throw new Error('missing nox grammar contribution');
}
if (!manifest.contributes.breakpoints.some((entry) => entry.language === 'nox')) {
  throw new Error('missing nox breakpoint contribution');
}
if (!manifest.contributes.debuggers.some((debuggerContribution) => debuggerContribution.type === 'nox')) {
  throw new Error('missing nox debugger contribution');
}
if (manifest.scripts.package.includes('--no-dependencies')) {
  throw new Error('package script must include runtime dependencies in the .vsix');
}

const grammar = JSON.parse(fs.readFileSync(path.join(root, 'syntaxes/nox.tmLanguage.json'), 'utf8'));
if (grammar.scopeName !== 'source.nox') {
  throw new Error('unexpected grammar scope');
}

const source = fs.readFileSync(path.join(root, 'src/extension.js'), 'utf8');
for (const expected of [
  'function activate',
  'function deactivate',
  "args: ['lsp']",
  "registerDebugAdapterDescriptorFactory('nox'",
  "['dap']",
  "return 'nox'"
]) {
  if (!source.includes(expected)) {
    throw new Error(`extension source missing ${expected}`);
  }
}

console.log('vscode-nox smoke ok');
