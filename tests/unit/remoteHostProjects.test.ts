import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'

const projects = readFileSync('src/composables/useProjects.ts', 'utf8')
const search = readFileSync('src/views/SearchView.vue', 'utf8')
const mcp = readFileSync('src/components/workshop/McpAddDialog.vue', 'utf8')

describe('projects belong to a single host', () => {
  it('keeps archive buckets apart when only the path matches across hosts', () => {
    expect(projects).toContain('return host ? `host:${host}|${pathKey}` : pathKey')
  })

  it('lists only local projects where remote ones cannot apply', () => {
    expect(search).toContain('projects.value.filter(project => !projectHost(project))')
    expect(search).toContain('v-for="p in searchableProjects"')
    expect(mcp).toContain('projects.value.filter(p => !projectHost(p))')
  })
})
