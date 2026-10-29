// Persistent-process adapter for measuring CodeGraph library search without CLI startup noise.

'use strict'

const fs = require('node:fs')
const path = require('node:path')

async function main() {
  const [libraryRoot, projectRoot, repetitionsText, limitText] = process.argv.slice(2)
  const repetitions = Number(repetitionsText)
  const limit = Number(limitText)
  const input = JSON.parse(fs.readFileSync(0, 'utf8'))
  const loadStarted = process.hrtime.bigint()
  const codegraphModule = require(path.join(libraryRoot, 'dist/index.js'))
  const generatedModule = require(path.join(
    libraryRoot,
    'dist/extraction/generated-detection.js',
  ))
  const { ToolHandler } = require(path.join(libraryRoot, 'dist/mcp/tools.js'))
  const CodeGraph = codegraphModule.CodeGraph || codegraphModule.default
  const graph = await CodeGraph.open(projectRoot, { sync: false, readOnly: true })
  const loadMs = millisecondsSince(loadStarted)
  const stats = graph.getStats()
  const handler = new ToolHandler(graph)
  const tracks = []
  for (const track of input.tracks) {
    const queries = []
    for (const query of track.queries) {
      queries.push(
        track.name !== 'symbol'
          ? await runExplore(handler, query, repetitions, limit)
          : runSearch(graph, generatedModule, query, repetitions, limit),
      )
    }
    tracks.push({ name: track.name, queries })
  }
  graph.close()
  process.stdout.write(JSON.stringify({ loadMs, stats, tracks }))
}

async function runExplore(handler, query, repetitions, limit) {
  const first = await explore(handler, query.query, limit)
  const durationsMs = []
  for (let index = 0; index < repetitions; index += 1) {
    const started = process.hrtime.bigint()
    await explore(handler, query.query, limit)
    durationsMs.push(millisecondsSince(started))
  }
  return { id: query.id, query: query.query, durationsMs, results: first }
}

async function explore(handler, query, limit) {
  const response = await handler.execute('codegraph_explore', { query, maxFiles: limit })
  if (response.isError) throw new Error(response.content?.[0]?.text || 'CodeGraph explore failed')
  return parseExplore(response.content?.[0]?.text || '')
}

function parseExplore(text) {
  const results = []
  let current = null
  let inCode = false
  for (const line of text.split('\n')) {
    const heading = line.match(/^####\s+(.+?)(?:\s+—|$)/)
    if (heading) {
      current = { path: heading[1].trim(), startLine: 0, endLine: 0, score: 0, lines: [] }
      results.push(current)
      inCode = false
      continue
    }
    if (line.startsWith('```')) {
      inCode = !inCode
      continue
    }
    if (!inCode || !current) continue
    const numbered = line.match(/^(\d+)\t/)
    if (!numbered) continue
    const number = Number(numbered[1])
    current.lines.push(number)
    if (current.startLine === 0) current.startLine = number
    current.endLine = number
  }
  return results
    .filter((result) => result.lines.length > 0)
    .map((result, index) => ({ ...result, score: 1 / (index + 1) }))
}

function runSearch(graph, generatedModule, query, repetitions, limit) {
  const durationsMs = []
  const first = sortedResults(graph, generatedModule, query.query, limit)
  for (let index = 0; index < repetitions; index += 1) {
    const started = process.hrtime.bigint()
    sortedResults(graph, generatedModule, query.query, limit)
    durationsMs.push(millisecondsSince(started))
  }
  return {
    id: query.id,
    query: query.query,
    durationsMs,
    results: first.map(({ node, score }) => ({
      path: node.filePath,
      startLine: node.startLine,
      endLine: node.endLine,
      score,
      lines: null,
    })),
  }
}

function sortedResults(graph, generatedModule, query, limit) {
  const raw = graph.searchNodes(query, { limit })
  return [...raw].sort((left, right) => {
    const leftGenerated = generatedModule.isGeneratedFile(left.node.filePath) ? 1 : 0
    const rightGenerated = generatedModule.isGeneratedFile(right.node.filePath) ? 1 : 0
    return leftGenerated - rightGenerated
  })
}

function millisecondsSince(started) {
  return Number(process.hrtime.bigint() - started) / 1_000_000
}

main().catch((error) => {
  process.stderr.write(`${error.stack || error}\n`)
  process.exitCode = 1
})
