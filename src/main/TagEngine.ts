// src/main/TagEngine.ts
import { app } from 'electron'
import sqlite3 from 'sqlite3'
import crypto from 'crypto'
import chokidar from 'chokidar'
import path from 'path'
import fs from 'fs'
import { getAudioDurationInSeconds } from 'get-audio-duration'
import * as mm from 'music-metadata'

// Enable verbose mode for debugging if needed
sqlite3.verbose()

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
  private db: sqlite3.Database

  constructor() {
    const dbPath = path.join(app.getPath('userData'), 'tags.db')
    this.db = new sqlite3.Database(dbPath, (err) => {
      if (err) {
        console.error('Database creation error:', err)
      } else {
        console.log('Connected to the database.')
        this.initializeDatabase().catch(console.error)
      }
    })
  }

  private initializeDatabase(): Promise<void> {
    return new Promise((resolve, reject) => {
      const schema = `
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
      `

      this.db.exec(schema, async (err) => {
        if (err) {
          reject(err)
          return
        }

        try {
          // Add default tags
          const defaultTags = ['favorite', 'loop', 'drum', 'bass', 'melody', 'fx']
          for (const tag of defaultTags) {
            await this.runQuery('INSERT OR IGNORE INTO tags (name) VALUES (?)', [tag])
          }
          resolve()
        } catch (error) {
          reject(error)
        }
      })
    })
  }

  // Helper function to wrap db.run in a Promise
  private runQuery(sql: string, params: any[] = []): Promise<void> {
    return new Promise((resolve, reject) => {
      this.db.run(sql, params, function (err) {
        if (err) reject(err)
        else resolve()
      })
    })
  }

  // Helper function to wrap db.get in a Promise
  private getQuery<T>(sql: string, params: any[] = []): Promise<T> {
    return new Promise((resolve, reject) => {
      this.db.get(sql, params, (err, row) => {
        if (err) reject(err)
        else resolve(row as T)
      })
    })
  }

  // Helper function to wrap db.all in a Promise
  private allQuery<T>(sql: string, params: any[] = []): Promise<T[]> {
    return new Promise((resolve, reject) => {
      this.db.all(sql, params, (err, rows) => {
        if (err) reject(err)
        else resolve(rows as T[])
      })
    })
  }

  private async calculateFileHash(filePath: string): Promise<string> {
    return new Promise((resolve, reject) => {
      const hash = crypto.createHash('sha1')
      const stream = fs.createReadStream(filePath)
      stream.on('error', (err) => reject(err))
      stream.on('data', (chunk) => hash.update(chunk))
      stream.on('end', () => resolve(hash.digest('hex')))
    })
  }

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

  async addFile(filePath: string): Promise<void> {
    try {
      const stats = await fs.promises.stat(filePath)
      const hash = await this.calculateFileHash(filePath)
      const metadata = await this.extractAudioMetadata(filePath)

      await this.db.serialize(async () => {
        await this.runQuery(
          'INSERT OR REPLACE INTO files (path, hash, last_modified) VALUES (?, ?, ?)',
          [filePath, hash, stats.mtimeMs]
        )

        const file = await this.getQuery<{ id: number }>('SELECT id FROM files WHERE path = ?', [
          filePath
        ])

        if (file) {
          await this.runQuery(
            `
            INSERT OR REPLACE INTO audio_metadata 
            (file_id, duration, sample_rate, channels, format, bitrate)
            VALUES (?, ?, ?, ?, ?, ?)
          `,
            [
              file.id,
              metadata.duration,
              metadata.sampleRate,
              metadata.channels,
              metadata.format,
              metadata.bitrate
            ]
          )
        }
      })
    } catch (error) {
      console.error(`Error adding file ${filePath}:`, error)
      throw error
    }
  }

  async addTags(tagNames: string[]): Promise<void> {
    for (const tag of tagNames) {
      await this.runQuery('INSERT OR IGNORE INTO tags (name) VALUES (?)', [tag])
    }
  }

  async tagFiles(filePaths: string[], tagName: string): Promise<void> {
    for (const filePath of filePaths) {
      await this.runQuery(
        `
        INSERT INTO file_tags (file_id, tag_id)
        SELECT f.id, t.id FROM files f, tags t 
        WHERE f.path = ? AND t.name = ?
      `,
        [filePath, tagName]
      )
    }
  }

  async untagFile(filePath: string, tagName: string): Promise<void> {
    await this.runQuery(
      `
      DELETE FROM file_tags 
      WHERE file_id IN (SELECT id FROM files WHERE path = ?)
      AND tag_id IN (SELECT id FROM tags WHERE name = ?)
    `,
      [filePath, tagName]
    )
  }

  async getFileMetadata(filePath: string): Promise<AudioFile | null> {
    return this.getQuery<AudioFile>(
      `
      SELECT f.*, 
             am.duration, am.sample_rate, am.channels, am.format, am.bitrate,
             GROUP_CONCAT(t.name) as tags
      FROM files f
      LEFT JOIN audio_metadata am ON am.file_id = f.id
      LEFT JOIN file_tags ft ON ft.file_id = f.id
      LEFT JOIN tags t ON t.id = ft.tag_id
      WHERE f.path = ?
      GROUP BY f.id
    `,
      [filePath]
    )
  }

  async searchByTags(tags: string[], options: TagSearchOptions = {}): Promise<AudioFile[]> {
    const { matchAll = true, sortBy = 'path', sortOrder = 'asc' } = options
    const operator = matchAll ? 'AND' : 'OR'
    const placeholders = tags.map(() => '?').join(` ${operator} t.name = `)

    return this.allQuery<AudioFile>(
      `
      SELECT DISTINCT f.*, 
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
      ORDER BY ${sortBy} ${sortOrder}
    `,
      tags
    )
  }

  watchDirectory(directory: string): chokidar.FSWatcher {
    const watcher = chokidar.watch(directory, {
      persistent: true,
      ignoreInitial: false,
      ignored: /(^|[\\/\\])\../,
      awaitWriteFinish: true
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
      .on('unlink', async (path) => {
        await this.runQuery('DELETE FROM files WHERE path = ?', [path])
      })
      .on('ready', () => {
        console.log('Initial scan complete')
      })
      .on('error', (error) => {
        console.error('Error watching directory:', error)
      })

    return watcher
  }

  private isAudioFile(filePath: string): boolean {
    const audioExtensions = ['.mp3', '.wav', '.ogg', '.m4a', '.flac', '.aiff']
    return audioExtensions.includes(path.extname(filePath).toLowerCase())
  }

  async getStats(): Promise<any> {
    const stats: any = {}

    const fileCount = await this.getQuery<{ count: number }>('SELECT COUNT(*) as count FROM files')
    stats.totalFiles = fileCount.count

    const tagCount = await this.getQuery<{ count: number }>('SELECT COUNT(*) as count FROM tags')
    stats.totalTags = tagCount.count

    stats.tagCounts = await this.allQuery(`
      SELECT t.name, COUNT(ft.file_id) as count 
      FROM tags t 
      LEFT JOIN file_tags ft ON ft.tag_id = t.id 
      GROUP BY t.id
    `)

    return stats
  }

  async close(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.db.close((err) => {
        if (err) reject(err)
        else resolve()
      })
    })
  }
}

export default TagEngine
