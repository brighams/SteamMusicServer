import { createServer } from 'http'
import { readFileSync, mkdirSync } from 'fs'
import { join, dirname } from 'path'
import { fileURLToPath } from 'url'
import Database from 'better-sqlite3'

const __dirname = dirname(fileURLToPath(import.meta.url))
const ROOT = join(__dirname, '..')
const PORT = 3737
const DB_PATH = join(ROOT, 'media/VSA_shaders.db')

const db = new Database(DB_PATH)


const get_shaders = db.prepare(`
  SELECT s.*, COALESCE(r.rating, 0) AS rating, r.foreground, r.background, r.mouse
  FROM vsa_shaders s
  LEFT JOIN vsa_ratings r ON r._id = s._id
  ORDER BY CASE COALESCE(r.rating, 0)
    WHEN 0  THEN 0
    WHEN -1 THEN 1
    WHEN 1  THEN 2
    WHEN 2  THEN 3
    WHEN 3  THEN 4
  END ASC, s.created_at DESC
`)

const upsert_rating = db.prepare(`
  INSERT INTO vsa_ratings (_id, rating, foreground, background, mouse)
  VALUES (@id, @rating, @foreground, @background, @mouse)
  ON CONFLICT(_id) DO UPDATE SET
    rating = excluded.rating,
    foreground = excluded.foreground,
    background = excluded.background,
    mouse = excluded.mouse
`)

const handle = async (req, res) => {
  if (req.method === 'GET' && req.url === '/') {
    res.writeHead(200, { 'Content-Type': 'text/html' })
    res.end(readFileSync(join(__dirname, 'index.html')))
    return
  }

  if (req.method === 'GET' && req.url === '/api/shaders') {
    res.writeHead(200, { 'Content-Type': 'application/json' })
    res.end(JSON.stringify(get_shaders.all()))
    return
  }

  if (req.method === 'POST' && req.url === '/api/rate') {
    let body = ''
    for await (const chunk of req) body += chunk
    const { id, rating, foreground, background, mouse } = JSON.parse(body)
    upsert_rating.run({ id, rating, foreground: foreground ?? null, background: background ?? null, mouse: mouse ?? null })
    res.writeHead(200, { 'Content-Type': 'application/json' })
    res.end(JSON.stringify({ ok: true }))
    return
  }

  res.writeHead(404)
  res.end()
}

createServer(handle).listen(PORT, () => console.log(`http://localhost:${PORT}`))
