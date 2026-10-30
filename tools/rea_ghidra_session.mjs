// Keep one REA MCP connection alive for several read-only Ghidra queries.
// CLI commands intentionally destroy their temporary database after each query.
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { mkdir, writeFile } from 'node:fs/promises';
import readline from 'node:readline';

const modules = process.argv[2];
if (!modules) throw new Error('Pass the installed npm node_modules directory.');
const { Client } = await import(pathToFileURL(path.join(modules, '@modelcontextprotocol/client/dist/index.mjs')));
const { StdioClientTransport } = await import(pathToFileURL(path.join(modules, '@modelcontextprotocol/client/dist/stdio.mjs')));
const outputRoot = path.resolve('docs/reference/ghidra-session');
await mkdir(outputRoot, { recursive: true });
const disabledAnalyzers = process.env.REA_GHIDRA_DISABLED_ANALYZERS ?? 'Create Address Tables,Decompiler Switch Analysis';
const transport = new StdioClientTransport({
  command: process.execPath,
  args: [path.join(modules, 'rea-agents/scripts/rea.mjs'), 'mcp'],
  cwd: process.cwd(),
  stderr: 'pipe',
  env: {
    ...process.env,
    GHIDRA_INSTALL_DIR: String.raw`C:\Users\Altieres\.codex\tools\ghidra\ghidra_12.1.4_PUBLIC`,
    REA_GHIDRA_STARTUP_TIMEOUT_MS: '2400000',
    REA_GHIDRA_DISABLED_ANALYZERS: disabledAnalyzers,
  },
});
const client = new Client({name: 'anycompare-reference-inspection', version: '0.1.5'});
const allowed = new Set(['open_binary', 'search_strings', 'search_procedures', 'xrefs',
  'procedure_pseudo_code', 'procedure_assembly', 'procedure_info', 'procedure_callees',
  'procedure_callers', 'read_bytes', 'resolve_containing_procedure',
  'analyze_function', 'binary_session', 'binary_overview', 'export_evidence_bundle',
  'import_evidence_bundle', 'close_binary']);
try {
  await client.connect(transport);
  console.log(JSON.stringify({state: 'ready', profile: 'focused', disabled_analyzers: disabledAnalyzers.split(',')}));
  for await (const line of readline.createInterface({input: process.stdin, crlfDelay: Infinity})) {
    if (!line.trim()) continue;
    const request = JSON.parse(line);
    if (request.quit) break;
    if (!allowed.has(request.name)) throw new Error('Only selected read-only analysis tools are allowed.');
    const output = path.resolve(outputRoot, request.output ?? `${request.name}.json`);
    if (!output.startsWith(outputRoot + path.sep)) throw new Error('Output must stay in the reference evidence folder.');
    console.log(JSON.stringify({state: 'running', tool: request.name, arguments: request.arguments}));
    try {
      const result = await client.callTool({name: request.name, arguments: request.arguments ?? {}}, {timeout: 2400000, maxTotalTimeout: 2400000});
      await writeFile(output, JSON.stringify(result, null, 2) + '\n');
      const evidence = result.structuredContent;
      console.log(JSON.stringify({state: 'complete', tool: request.name, output, isError: result.isError ?? false, evidence_id: evidence?.evidence_id}));
    } catch (error) { console.log(JSON.stringify({state: 'failed', tool: request.name, error: String(error)})); }
  }
} finally { await client.close(); }
