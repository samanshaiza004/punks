// src/main/TagEngine.ts
import { app } from 'electron'
import * as sqlite3 from 'sqlite3'

import fs from 'fs'
import path from 'path'
import { TagSearchOptions, TagEngineEvents } from '../types'
import crypto from 'crypto'
import chokidar from 'chokidar'
import { EventEmitter } from 'events'

sqlite3.verbose()

const CREATE_TABLES_SQL = `
  -- Directories table to store folder structure
  CREATE TABLE IF NOT EXISTS directories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT UNIQUE NOT NULL,
    parent_path TEXT,
    name TEXT NOT NULL,
    last_modified INTEGER NOT NULL,
    created_at INTEGER DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER DEFAULT (strftime('%s', 'now'))
  );

  -- Files table with directory reference
  CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT UNIQUE NOT NULL,
    directory_path TEXT NOT NULL,
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    hash TEXT NOT NULL,
    last_modified INTEGER NOT NULL,
    created_at INTEGER DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER DEFAULT (strftime('%s', 'now')),
    FOREIGN KEY (directory_path) REFERENCES directories(path) ON DELETE CASCADE
  );

  -- Tags table
  CREATE TABLE IF NOT EXISTS tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT UNIQUE NOT NULL,
    created_at INTEGER DEFAULT (strftime('%s', 'now'))
  );

  -- File tags junction table
  CREATE TABLE IF NOT EXISTS file_tags (
    file_id INTEGER,
    tag_id INTEGER,
    created_at INTEGER DEFAULT (strftime('%s', 'now')),
    PRIMARY KEY (file_id, tag_id),
    FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
  );

  -- Create indexes
  CREATE INDEX IF NOT EXISTS idx_directories_path ON directories(path);
  CREATE INDEX IF NOT EXISTS idx_directories_parent_path ON directories(parent_path);
  CREATE INDEX IF NOT EXISTS idx_files_path ON files(path);
  CREATE INDEX IF NOT EXISTS idx_files_directory_path ON files(directory_path);
  CREATE INDEX IF NOT EXISTS idx_files_type ON files(type);
  CREATE INDEX IF NOT EXISTS idx_files_hash ON files(hash);
  CREATE INDEX IF NOT EXISTS idx_tags_name ON tags(name);
`;

class TagEngine extends EventEmitter<TagEngineEvents> {
  removeTag(): any {
    throw new Error('Method not implemented.')
  }
  getAllTags(): any {
    throw new Error('Method not implemented.')
  }
  async addTag(filePath: string, tag: string): Promise<void> {
    return new Promise((resolve, reject) => {
      try {
        this.db.get('SELECT id FROM files WHERE path = ?', [filePath], (err, fileRow: any) => {
          if (err) {
            this.logger.error('Error checking file:', err)
            return reject(err)
          }

          if (!fileRow) {
            this.logger.error('File not found in database:', filePath)
            return reject(new Error('File not found'))
          }

          this.db.run('INSERT OR IGNORE INTO tags (name) VALUES (?)', [tag], (tagErr) => {
            if (tagErr) {
              this.logger.error('Error inserting tag:', tagErr)
              return reject(tagErr)
            }

            this.db.get('SELECT id FROM tags WHERE name = ?', [tag], (getTagErr, tagRow: any) => {
              if (getTagErr) {
                this.logger.error('Error retrieving tag:', getTagErr)
                return reject(getTagErr)
              }

              this.db.run(
                'INSERT OR IGNORE INTO file_tags (file_id, tag_id) VALUES (?, ?)', 
                [fileRow.id, tagRow.id], 
                (insertErr) => {
                  if (insertErr) {
                    this.logger.error('Error adding tag to file:', insertErr)
                    return reject(insertErr)
                  }
                  resolve()
                }
              )
            })
          })
        })
      } catch (error) {
        this.logger.error('Unexpected error in addTag:', error)
        reject(error)
      }
    })
  }
  private db: sqlite3.Database
  private logger: Console

  constructor() {
    super()
    this.logger = console
    const dbPath = path.join(app.getPath('userData'), 'tags.db')
    
    this.logger.info('Initializing TagEngine with database at:', dbPath)
    
    try {
      this.db = new sqlite3.Database(dbPath, (err) => {
        if (err) {
          this.logger.error('Database creation error:', err)
          throw err
        }
        this.logger.info('Successfully connected to database')
        this.initializeDatabase()
      })
    } catch (error) {
      this.logger.error('Failed to create database:', error)
      throw error
    }
  }

  private initializeDatabase(): void {
    this.logger.info('Initializing database tables...')
    
    this.db.serialize(() => {
      this.db.exec(CREATE_TABLES_SQL, (err) => {
        if (err) {
          this.logger.error('Failed to create database tables:', err)
          throw err
        }
        this.logger.info('Database tables initialized successfully')
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

  async addFile(filePath: string): Promise<void> {
    try {
      const stats = await fs.promises.stat(filePath)
      const hash = await this.calculateFileHash(filePath)

      await this.db.serialize(async () => {
        await this.runQuery(
          'INSERT OR REPLACE INTO files (path, directory_path, name, type, hash, last_modified) VALUES (?, ?, ?, ?, ?, ?)',
          [filePath, path.dirname(filePath), path.basename(filePath), this.getFileType(filePath), hash, stats.mtimeMs]
        )
      })
    } catch (error) {
      this.logger.error(`Error adding file ${filePath}:`, error)
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

  async getFileMetadata(filePath: string): Promise<any | null> {
    return this.getQuery<any>(
      `
      SELECT f.*, 
             GROUP_CONCAT(t.name) as tags
      FROM files f
      LEFT JOIN file_tags ft ON ft.file_id = f.id
      LEFT JOIN tags t ON t.id = ft.tag_id
      WHERE f.path = ?
      GROUP BY f.id
    `,
      [filePath]
    )
  }

  async searchByTags(tags: string[], options: TagSearchOptions = {}): Promise<any[]> {
    const { matchAll = false, sortBy = 'path', sortOrder = 'asc' } = options
    
    // If no tags provided, return all files
    if (tags.length === 0) {
      return this.allQuery<any>(
        `
        SELECT DISTINCT f.*, 
                       GROUP_CONCAT(t.name) as tags
        FROM files f
        LEFT JOIN file_tags ft ON ft.file_id = f.id
        LEFT JOIN tags t ON t.id = ft.tag_id
        GROUP BY f.id
        ORDER BY ${sortBy} ${sortOrder}
        `
      )
    }

    const operator = matchAll ? 'AND' : 'OR'
    const placeholders = tags.map(() => '?').join(` ${operator} t.name = `)

    return this.allQuery<any>(
      `
      SELECT DISTINCT f.*, 
                     GROUP_CONCAT(t2.name) as tags
      FROM files f
      JOIN file_tags ft ON ft.file_id = f.id
      JOIN tags t ON t.id = ft.tag_id
      LEFT JOIN file_tags ft2 ON ft2.file_id = f.id
      LEFT JOIN tags t2 ON t2.id = ft2.tag_id
      WHERE t.name = ${placeholders}
      GROUP BY f.id
      ORDER BY ${sortBy} ${sortOrder}
      `,
      tags
    )
  }

  async scanDirectory(directoryPath: string): Promise<void> {
    this.logger.info('Starting directory scan:', directoryPath)
    
    try {
      const watcher = chokidar.watch(directoryPath, {
        ignored: /(^|[\/\\])\../, // ignore dotfiles
        persistent: false,
        depth: Infinity
      })

      let processedFiles = 0
      let totalFiles = 0

      // Process directories first
      const processDirectory = async (dirPath: string) => {
        const stats = fs.statSync(dirPath)
        const name = path.basename(dirPath)
        const parentPath = path.dirname(dirPath)

        await this.db.run(
          `INSERT OR REPLACE INTO directories 
           (path, parent_path, name, last_modified) 
           VALUES (?, ?, ?, ?)`,
          [dirPath, parentPath === dirPath ? null : parentPath, name, stats.mtimeMs]
        )
      }

      watcher.on('addDir', async (dirPath) => {
        try {
          await processDirectory(dirPath)
        } catch (error) {
          this.logger.error(`Error processing directory ${dirPath}:`, error)
        }
      })

      watcher.on('add', async (filePath) => {
        totalFiles++
        this.emit('scanProgress', {
          total: totalFiles,
          processed: processedFiles,
          percentComplete: totalFiles > 0 ? Math.round((processedFiles / totalFiles) * 100) : 0
        })

        try {
          await this.processFile(filePath)
          processedFiles++
          this.logger.debug(`Processed file ${processedFiles}/${totalFiles}: ${filePath}`)
        } catch (error) {
          this.logger.error(`Error processing file ${filePath}:`, error)
        }
      })

      await new Promise<void>((resolve, reject) => {
        watcher.on('ready', () => {
          watcher.close()
          this.logger.info(`Scan complete. Processed $ {processedFiles}/${totalFiles} files`)
          this.emit('scanComplete', { 
            totalFiles: processedFiles,
            directory: directoryPath
          })
          resolve()
        })

        watcher.on('error', (error: any) => {
          this.logger.error('Watcher error:', error)
          this.emit('error', error)
          reject(error)
        })
      })
    } catch (error) {
      this.logger.error('Scan error:', error)
      this.emit('error', error as Error)

      throw error
    }
  }

  private async processFile(filePath: string): Promise<void> {
    this.logger.debug('Processing file:', filePath)
    
    try {
      const hash = await this.calculateFileHash(filePath)
      const stats = fs.statSync(filePath)
      const name = path.basename(filePath)
      const directoryPath = path.dirname(filePath)

      const insertQuery = `
        INSERT OR REPLACE INTO files 
        (path, directory_path, name, type, hash, last_modified) 
        VALUES (?, ?, ?, ?, ?, ?)
      `
      
      await new Promise<void>((resolve, reject) => {
        this.db.run(insertQuery, [
          filePath,
          directoryPath,
          name,
          this.getFileType(filePath),
          hash,
          stats.mtime.getTime()
        ], (err) => {
          if (err) {
            this.logger.error('Error inserting file:', err)
            reject(err)
          } else {
            resolve()
          }
        })
      })

      this.logger.debug('Successfully processed file:', filePath)
    } catch (error) {
      this.logger.error('Error processing file:', filePath, error)
      throw error
    }
  }

  // Get contents of a directory
  async getDirectoryContents(directoryPath: string[]): Promise<{
    directories: Array<{ path: string; name: string; lastModified: number; type: 'directory' }>;
    files: any[];
    currentPath: string;
  }> {
    if (!Array.isArray(directoryPath)) {
      throw new Error('directoryPath must be an array of path segments')
    }
    
    const fullPath = directoryPath.length > 0 ? path.join(...directoryPath) : '/'
    
    try {
      const directoriesQuery = `
        SELECT path, name, last_modified as lastModified
        FROM directories
        WHERE parent_path = ?
        ORDER BY name COLLATE NOCASE ASC
      `
      const directories = await this.allQuery(directoriesQuery, [fullPath])

      const processedDirectories = directories.map((dir: any) => ({
        path: dir.path || '',
        name: dir.name || '',
        lastModified: dir.lastModified || Date.now(),
        type: 'directory' as const
      }))

      const filesQuery = `
        SELECT f.id, f.path, f.directory_path, f.name, f.type, f.hash, f.last_modified,
               f.created_at, f.updated_at,
               GROUP_CONCAT(t.name) as tags
        FROM files f
        LEFT JOIN file_tags ft ON f.id = ft.file_id
        LEFT JOIN tags t ON ft.tag_id = t.id
        WHERE f.directory_path = ?
        GROUP BY f.id
        ORDER BY f.name COLLATE NOCASE ASC
      `
      const files = await this.allQuery(filesQuery, [fullPath])
      
      const processedFiles = files.map((file: any) => ({
        id: file.id,
        path: file.directory_path,
        name: file.name,
        last_modified: file.last_modified,
        type: 'file',
        directory_path: file.directory_path,
        hash: '', // do i need this?
        tags: file.tags ? file.tags.split(',') : []
      }))

      return {
        directories: processedDirectories,
        files: processedFiles,
        currentPath: fullPath
      }
    } catch (error) {
      this.logger.error('Error getting directory contents:', error)
      throw error
    }
  }

  async getParentDirectory(currentPath: string): Promise<string | null> {
    try {
      const result = await this.getQuery<{ parent_path: string | null }>(
        'SELECT parent_path FROM directories WHERE path = ?',
        [currentPath]
      )
      return result?.parent_path || null
    } catch (error) {
      this.logger.error('Error getting parent directory:', error)
      throw error
    }
  }

  private getFileType(filePath: string): string {
    return path.extname(filePath).toLowerCase()
  }

  private runQuery(sql: string, params: any[] = []): Promise<void> {
    return new Promise((resolve, reject) => {
      this.db.run(sql, params, function (err) {
        if (err) reject(err)
        else resolve()
      })
    })
  }

  private getQuery<T>(sql: string, params: any[] = []): Promise<T> {
    return new Promise((resolve, reject) => {
      this.db.get(sql, params, (err, row) => {
        if (err) reject(err)
        else resolve(row as T)
      })
    })
  }

  private allQuery<T>(sql: string, params: any[] = []): Promise<T[]> {
    return new Promise((resolve, reject) => {
      this.db.all(sql, params, (err, rows) => {
        if (err) reject(err)
        else resolve(rows as T[])
      })
    })
  }

  async getStats(): Promise<{
    totalFiles: number
    totalTags: number
    tagCounts: { name: string; count: number }[]
  }> {
    const [{ totalFiles }] = await this.allQuery<{ totalFiles: number }>(
      'SELECT COUNT(*) as totalFiles FROM files'
    )
    const [{ totalTags }] = await this.allQuery<{ totalTags: number }>(
      'SELECT COUNT(*) as totalTags FROM tags'
    )
    const tagCounts = await this.allQuery<{ name: string; count: number }>(
      `
      SELECT t.name, COUNT(ft.file_id) as count
      FROM tags t
      LEFT JOIN file_tags ft ON ft.tag_id = t.id
      GROUP BY t.id
      ORDER BY count DESC
      `
    )

    return {
      totalFiles,
      totalTags,
      tagCounts
    }
  }

  async close(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.db.close((err) => {
        if (err) reject(err)
        else resolve()
      })
    })
  }

  watchDirectory(directory: string): chokidar.FSWatcher {
    const watcher = chokidar.watch(directory, {
      persistent: true,
      ignoreInitial: false,
      ignored: /(^|[/\\])\../,
      awaitWriteFinish: true,
      depth: undefined,
      followSymlinks: false
    })

    let initialScanComplete = false
    let filesProcessed = 0
    const totalFiles = new Set<string>()

    watcher
      .on('add', async (path) => {
        totalFiles.add(path)
        try {
          await this.addFile(path)
          filesProcessed++
          
          if (!initialScanComplete) {
            const progress = (filesProcessed / totalFiles.size) * 100
            this.emit('scanProgress', {
              total: totalFiles.size,
              processed: filesProcessed,
              percentComplete: Math.round(progress)
            })
          }
        } catch (error) {
          this.logger.error(`Error processing file ${path}:`, error)
        }
      })
      .on('change', async (path) => {
        try {
          await this.addFile(path)
          this.emit('fileChanged', path)
        } catch (error) {
          this.logger.error(`Error updating file ${path}:`, error)
        }
      })
      .on('unlink', async (path) => {
        try {
          await this.runQuery('DELETE FROM files WHERE path = ?', [path])
          this.emit('fileRemoved', path)
        } catch (error) {
          this.logger.error(`Error removing file ${path}:`, error)
        }
      })
      .on('ready', () => {
        console.log(`Initial scan complete. Processed ${filesProcessed} files.`)
        initialScanComplete = true
        this.emit('scanComplete', {
          totalFiles: filesProcessed,
          directory
        })
      })
      .on('error', (error) => {
        this.logger.error('Error watching directory:', error)
        this.emit('error', error)
      })

    return watcher
  }
}

export default TagEngine
