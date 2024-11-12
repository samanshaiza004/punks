/* eslint-disable @typescript-eslint/no-explicit-any */
// Using better-sqlite3 for performance
const Database = require('better-sqlite3')
const crypto = require('crypto')
const chokidar = require('chokidar')
const { app } = require('electron')
const path = require('path')

import fs from 'fs'

class TagEngine {
  db: any
  addFileStmt: any
  addTagStmt: any
  tagFileStmt: any
  untagFileStmt: any
  constructor() {
    const dbPath = path.join(app.getPath('userData'), 'tags.db')
    this.db = new Database(dbPath)

    // Initialize database
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS files (
        id INTEGER PRIMARY KEY,
        path TEXT UNIQUE,
        hash TEXT,
        last_modified INTEGER
      );

      CREATE TABLE IF NOT EXISTS tags (
        id INTEGER PRIMARY KEY,
        name TEXT UNIQUE
      );

      CREATE TABLE IF NOT EXISTS file_tags (
        file_id INTEGER,
        tag_id INTEGER,
        FOREIGN KEY(file_id) REFERENCES files(id),
        FOREIGN KEY(tag_id) REFERENCES tags(id),
        PRIMARY KEY(file_id, tag_id)
      );

      -- Add default favorite tag
      INSERT OR IGNORE INTO tags (name) VALUES ('favorite');
    `)

    // Prepare statements
    this.addFileStmt = this.db.prepare(
      'INSERT OR REPLACE INTO files (path, hash, last_modified) VALUES (?, ?, ?)'
    )
    this.addTagStmt = this.db.prepare('INSERT OR IGNORE INTO tags (name) VALUES (?)')
    this.tagFileStmt = this.db.prepare(
      `INSERT INTO file_tags (file_id, tag_id)
       SELECT f.id, t.id FROM files f, tags t 
       WHERE f.path = ? AND t.name = ?`
    )
    this.untagFileStmt = this.db.prepare(
      `DELETE FROM file_tags 
       WHERE file_id IN (SELECT id FROM files WHERE path = ?)
       AND tag_id IN (SELECT id FROM tags WHERE name = ?)`
    )
  }

  // Calculate file hash for tracking moves
  async calculateFileHash(filePath): Promise<string> {
    return new Promise((resolve, reject) => {
      const hash = crypto.createHash('sha1')
      const stream = fs.createReadStream(filePath)
      stream.on('error', (err) => reject(err))
      stream.on('data', (chunk) => hash.update(chunk))
      stream.on('end', () => resolve(hash.digest('hex')))
    })
  }

  // Add or update file in database
  async addFile(filePath): Promise<void> {
    const stats = await fs.promises.stat(filePath)
    const hash = await this.calculateFileHash(filePath)
    this.addFileStmt.run(filePath, hash, stats.mtimeMs)
  }

  // Add a new tag type
  addTag(tagName): void {
    this.addTagStmt.run(tagName)
  }

  // Tag a file
  tagFile(filePath, tagName): void {
    this.tagFileStmt.run(filePath, tagName)
  }

  // Remove a tag from a file
  untagFile(filePath, tagName): void {
    this.untagFileStmt.run(filePath, tagName)
  }

  // Get all tags for a file
  getFileTags(filePath): void {
    return this.db
      .prepare(
        `SELECT t.name 
       FROM tags t
       JOIN file_tags ft ON ft.tag_id = t.id
       JOIN files f ON f.id = ft.file_id
       WHERE f.path = ?`
      )
      .all(filePath)
  }

  // Watch for file system changes
  watchDirectory(directory): void {
    const watcher = chokidar.watch(directory, {
      persistent: true,
      ignoreInitial: false
    })

    watcher
      .on('add', (path) => this.addFile(path))
      .on('change', (path) => this.addFile(path))
      .on('unlink', (path) => {
        // Handle file deletion
        this.db.prepare('DELETE FROM files WHERE path = ?').run(path)
      })
      .on('ready', () => {
        console.log('Initial scan complete')
      })

    return watcher
  }

  // Search files by tags
  searchByTags(tags): void {
    const placeholders = tags.map(() => '?').join(',')
    return this.db
      .prepare(
        `SELECT DISTINCT f.path 
       FROM files f
       JOIN file_tags ft ON ft.file_id = f.id
       JOIN tags t ON t.id = ft.tag_id
       WHERE t.name IN (${placeholders})
       GROUP BY f.id
       HAVING COUNT(DISTINCT t.id) = ?`
      )
      .all(...tags, tags.length)
  }
}

export default TagEngine
