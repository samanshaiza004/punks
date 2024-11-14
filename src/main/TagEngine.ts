/* eslint-disable @typescript-eslint/no-explicit-any */

import { app } from 'electron'
import Database from 'better-sqlite3'
import crypto from 'crypto'
import chokidar from 'chokidar'
import path from 'path'
import fs from 'fs'
import { getAudioDurationInSeconds } from 'get-audio-duration'
import * as mm from 'music-metadata'

interface AudioMetadata {
  duration: number
  sampleRate?: number
  channels?: number
  format?: string
  bitrate?: number
}

interface AudioFile {
  id: number
  path: string
  hash: string
  lastModified: number
  metadata: AudioMetadata
  tags: string[]
}

interface TagSearchOptions {
  matchAll?: boolean
  sortBy?: 'path' | 'lastModified' | 'duration'
  sortOrder?: 'asc' | 'desc'
}

class TagEngine {
  private db: any
  private addFileStmt: any
  private addTagStmt: any
  private tagFileStmt: any
  private untagFileStmt: any
  private addMetadataStmt: any

  constructor() {
    const dbPath = path.join(app.getPath('userData'), 'tags.db')
    this.db = new Database(dbPath)

    // Initialize database with enhanced schema
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

      CREATE TABLE IF NOT EXISTS audio_metadata (
        file_id INTEGER PRIMARY KEY,
        duration REAL,
        sample_rate INTEGER,
        channels INTEGER,
        format TEXT,
        bitrate INTEGER,
        FOREIGN KEY(file_id) REFERENCES files(id)
      );

      -- Add default tags
      INSERT OR IGNORE INTO tags (name) VALUES 
        ('favorite'),
        ('loop'),
        ('drum'),
        ('bass'),
        ('melody'),
        ('fx');
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
    this.addMetadataStmt = this.db.prepare(
      `INSERT OR REPLACE INTO audio_metadata 
       (file_id, duration, sample_rate, channels, format, bitrate)
       SELECT id, ?, ?, ?, ?, ? FROM files WHERE path = ?`
    )
  }

  // Calculate file hash for tracking moves
  private async calculateFileHash(filePath: string): Promise<string> {
    return new Promise((resolve, reject) => {
      const hash = crypto.createHash('sha1')
      const stream = fs.createReadStream(filePath)
      stream.on('error', (err) => reject(err))
      stream.on('data', (chunk) => hash.update(chunk))
      stream.on('end', () => resolve(hash.digest('hex')))
    })
  }

  // Extract audio metadata using music-metadata
  private async extractAudioMetadata(filePath: string): Promise<AudioMetadata> {
    try {
      const metadata = await mm.parseFile(filePath)
      const duration = await getAudioDurationInSeconds(filePath)
      return {
        duration,
        sampleRate: metadata.format.sampleRate,
        channels: metadata.format.numberOfChannels,
        format: metadata.format.container,
        bitrate: metadata.format.bitrate
      }
    } catch (error) {
      console.error(`Error extracting metadata for ${filePath}:`, error)
      return { duration: 0 }
    }
  }

  // Add or update file in database with metadata
  async addFile(filePath: string): Promise<void> {
    try {
      const stats = await fs.promises.stat(filePath)
      const hash = await this.calculateFileHash(filePath)
      const metadata = await this.extractAudioMetadata(filePath)

      this.db.transaction(() => {
        this.addFileStmt.run(filePath, hash, stats.mtimeMs)
        this.addMetadataStmt.run(
          metadata.duration,
          metadata.sampleRate,
          metadata.channels,
          metadata.format,
          metadata.bitrate,
          filePath
        )
      })()
    } catch (error) {
      console.error(`Error adding file ${filePath}:`, error)
      throw error
    }
  }

  // Add multiple tags at once
  async addTags(tagNames: string[]): Promise<void> {
    this.db.transaction(() => {
      tagNames.forEach((tag) => this.addTagStmt.run(tag))
    })()
  }

  // Tag multiple files at once
  async tagFiles(filePaths: string[], tagName: string): Promise<void> {
    this.db.transaction(() => {
      filePaths.forEach((path) => this.tagFileStmt.run(path, tagName))
    })()
  }

  async untagFile(filePath: string, tagName: string): Promise<void> {
    this.db.transaction(() => {
      this.untagFileStmt.run(filePath, tagName)
    })()
  }

  // Get all metadata for a file
  getFileMetadata(filePath: string): AudioFile | null {
    return this.db
      .prepare(
        `SELECT f.*, 
                am.duration, am.sample_rate, am.channels, am.format, am.bitrate,
                GROUP_CONCAT(t.name) as tags
         FROM files f
         LEFT JOIN audio_metadata am ON am.file_id = f.id
         LEFT JOIN file_tags ft ON ft.file_id = f.id
         LEFT JOIN tags t ON t.id = ft.tag_id
         WHERE f.path = ?
         GROUP BY f.id`
      )
      .get(filePath)
  }

  // Enhanced search with options
  searchByTags(tags: string[], options: TagSearchOptions = {}): AudioFile[] {
    const { matchAll = true, sortBy = 'path', sortOrder = 'asc' } = options
    const operator = matchAll ? 'AND' : 'OR'
    const placeholders = tags.map(() => '?').join(` ${operator} t.name = `)

    return this.db
      .prepare(
        `SELECT DISTINCT f.*, 
                        am.duration, am.sample_rate, am.channels, am.format, am.bitrate,
                        GROUP_CONCAT(t2.name) as file_tags
         FROM files f
         JOIN file_tags ft ON ft.file_id = f.id
         JOIN tags t ON t.id = ft.tag_id
         LEFT JOIN audio_metadata am ON am.file_id = f.id
         LEFT JOIN file_tags ft2 ON ft2.file_id = f.id
         LEFT JOIN tags t2 ON t2.id = ft2.tag_id
         WHERE t.name = ${placeholders}
         GROUP BY f.id
         ORDER BY ${sortBy} ${sortOrder}`
      )
      .all(tags)
  }

  // Watch for file system changes
  watchDirectory(directory: string): chokidar.FSWatcher {
    const watcher = chokidar.watch(directory, {
      persistent: true,
      ignoreInitial: false,
      ignored: /(^|[\\/\\])\../, // Ignore hidden files
      awaitWriteFinish: true // Wait for writes to finish
    })

    watcher
      .on('add', async (path) => {
        if (this.isAudioFile(path)) {
          await this.addFile(path)
        }
      })
      .on('change', async (path) => {
        if (this.isAudioFile(path)) {
          await this.addFile(path)
        }
      })
      .on('unlink', (path) => {
        this.db.prepare('DELETE FROM files WHERE path = ?').run(path)
      })
      .on('ready', () => {
        console.log('Initial scan complete')
      })
      .on('error', (error) => {
        console.error('Error watching directory:', error)
      })

    return watcher
  }

  // Helper to check if file is an audio file
  private isAudioFile(filePath: string): boolean {
    const audioExtensions = ['.mp3', '.wav', '.ogg', '.m4a', '.flac', '.aiff']
    return audioExtensions.includes(path.extname(filePath).toLowerCase())
  }

  // Get statistics about the database
  getStats(): any {
    return {
      totalFiles: this.db.prepare('SELECT COUNT(*) as count FROM files').get().count,
      totalTags: this.db.prepare('SELECT COUNT(*) as count FROM tags').get().count,
      tagCounts: this.db
        .prepare(
          `SELECT t.name, COUNT(ft.file_id) as count 
         FROM tags t 
         LEFT JOIN file_tags ft ON ft.tag_id = t.id 
         GROUP BY t.id`
        )
        .all()
    }
  }
}

export default TagEngine
