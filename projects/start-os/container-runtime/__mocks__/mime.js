// Mock for ESM-only mime package — Jest's module loader doesn't support require(esm)
const types = {
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.gif': 'image/gif',
  '.svg': 'image/svg+xml',
  '.webp': 'image/webp',
  '.ico': 'image/x-icon',
  '.json': 'application/json',
  '.js': 'application/javascript',
  '.html': 'text/html',
  '.css': 'text/css',
  '.txt': 'text/plain',
  '.md': 'text/markdown',
}

module.exports = {
  default: {
    getType(path) {
      const ext = '.' + path.split('.').pop()
      return types[ext] || null
    },
    getExtension(type) {
      const entry = Object.entries(types).find(([, v]) => v === type)
      return entry ? entry[0].slice(1) : null
    },
  },
  __esModule: true,
}
