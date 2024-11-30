import React, { useState, useEffect, useCallback } from 'react';
import { ArrowRight, ArrowDown, Folder } from '@phosphor-icons/react';
import { useTheme } from '@renderer/context/ThemeContext';
import { useTabs } from '@renderer/context/TabContext';
import { useUISettings } from '@renderer/context/UISettingsContext';

interface FolderNode {
  name: string;
  path: string;
  children: FolderNode[];
  isExpanded?: boolean;
  hasChildren?: boolean;
}

const FolderTreeItem: React.FC<{
  folder: FolderNode;
  level: number;
  onToggle: (path: string) => void;
  onSelect: (path: string) => void;
  selectedPath: string | null;
}> = ({ folder, level, onToggle, onSelect, selectedPath }) => {
  const { isDarkMode } = useTheme();
  const { getSizeClass } = useUISettings();
  const isSelected = selectedPath === folder.path;

  const handleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    onSelect(folder.path);
  };

  const handleToggle = (e: React.MouseEvent) => {
    e.stopPropagation();
    onToggle(folder.path);
  };

  return (
    <div className="select-none">
      <div
        className={`
          flex items-center cursor-pointer
          ${getSizeClass('folder')}
          ${isSelected 
            ? 'bg-blue-500 text-white' 
            : isDarkMode 
              ? 'text-gray-200 hover:bg-gray-700' 
              : 'text-gray-800 hover:bg-gray-100'
          }
        `}
        style={{ paddingLeft: `${level * 12 + 8}px` }}
        onClick={handleClick}
      >
        <div
          className="p-1 hover:bg-opacity-20 hover:bg-gray-500 rounded"
          onClick={handleToggle}
        >
          {(folder.children.length > 0 || folder.hasChildren) && (
            folder.isExpanded ? (
              <ArrowDown className="w-4 h-4" />
            ) : (
              <ArrowRight className="w-4 h-4" />
            )
          )}
        </div>
        <Folder className="min-w-4 min-h-4 mx-2" />
        <span className="truncate">{folder.name}</span>
      </div>
      {folder.isExpanded && folder.children.map((child) => (
        <FolderTreeItem
          key={child.path}
          folder={child}
          level={level + 1}
          onToggle={onToggle}
          onSelect={onSelect}
          selectedPath={selectedPath}
        />
      ))}
    </div>
  );
};

export const FolderTree: React.FC = () => {
  const [folderStructure, setFolderStructure] = useState<FolderNode | null>(null);
  const { state, dispatch } = useTabs();
  const activeTab = state.tabs.find(t => t.id === state.activeTabId);
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set());
  const [rootPath, setRootPath] = useState<string | null>(null);

  
  const buildFolderStructure = useCallback(async (path: string): Promise<FolderNode | null> => {
    try {
      const contents = await window.api.getDirectoryContents([path]);
      const directories = contents.directories.sort((a, b) => a.name.localeCompare(b.name));
      
      const children = await Promise.all(
        directories.map(async (dir) => {
          // Check if this directory has any subdirectories without loading them all
          const dirContents = await window.api.getDirectoryContents([dir.path]);
          return {
            name: dir.name,
            path: dir.path,
            children: [], // Still empty, but we'll load them when expanded
            isExpanded: expandedFolders.has(dir.path),
            hasChildren: dirContents.directories.length > 0 // Add this flag
          };
        })
      );

      return {
        name: path.split('/').pop() || path,
        path: path,
        children: children,
        isExpanded: expandedFolders.has(path),
        hasChildren: children.length > 0
      };
    } catch (error) {
      console.error('Error building folder structure:', error);
      return null;
    }
  }, [expandedFolders]);

  const loadChildrenForFolder = useCallback(async (folder: FolderNode): Promise<FolderNode> => {
    try {
      const contents = await window.api.getDirectoryContents([folder.path]);
      const directories = contents.directories.sort((a, b) => a.name.localeCompare(b.name));
      
      const children = await Promise.all(directories.map(async (dir) => {
        const dirContents = await window.api.getDirectoryContents([dir.path]);
        return {
          name: dir.name,
          path: dir.path,
          children: [], // Start with empty children
          isExpanded: expandedFolders.has(dir.path),
          hasChildren: dirContents.directories.length > 0
        };
      }));

      return {
        ...folder,
        children: children,
        hasChildren: children.length > 0
      };
    } catch (error) {
      console.error('Error loading children:', error);
      return folder;
    }
  }, [expandedFolders]);

  useEffect(() => {
    const newRootPath = activeTab?.directoryPath[0];
    if (newRootPath && newRootPath !== rootPath) {
      setRootPath(newRootPath);
      const initializeFolderStructure = async () => {
        const structure = await buildFolderStructure(newRootPath);
        setFolderStructure(structure);
      };
      initializeFolderStructure();
    }
  }, [activeTab?.directoryPath[0], buildFolderStructure, rootPath]);

  const handleToggle = useCallback((path: string) => {
    setFolderStructure(prev => {
      if (!prev) return prev;

      const updateFolderChildren = (folder: FolderNode): FolderNode => {
        if (folder.path === path) {
          // If this is the folder being toggled
          const newIsExpanded = !folder.isExpanded;
          
          if (newIsExpanded) {
            // If we're expanding, load children asynchronously
            loadChildrenForFolder(folder).then(updatedFolder => {
              setFolderStructure(currentStructure => 
                currentStructure ? updateFolderInTree(currentStructure, {
                  ...updatedFolder,
                  isExpanded: true
                }) : currentStructure
              );
            });
          }
          
          // Return immediately with just the expansion state changed
          return {
            ...folder,
            isExpanded: newIsExpanded
          };
        }

        // Recursively update children
        return {
          ...folder,
          children: folder.children.map(updateFolderChildren)
        };
      };

      return updateFolderInTree(prev, updateFolderChildren(prev));
    });

    // Update expanded folders set
    setExpandedFolders(prev => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }, [loadChildrenForFolder]);

  const updateFolderInTree = (
    tree: FolderNode, 
    updatedFolder: FolderNode | ((folder: FolderNode) => FolderNode)
  ): FolderNode => {
    const updateFolder = (folder: FolderNode): FolderNode => {
      if (folder.path === (typeof updatedFolder === 'function' ? 
        updatedFolder(folder).path : 
        updatedFolder.path)) {
        return typeof updatedFolder === 'function' 
          ? updatedFolder(folder) 
          : updatedFolder;
      }

      return {
        ...folder,
        children: folder.children.map(updateFolder)
      };
    };

    return updateFolder(tree);
  };

  const handleSelect = useCallback((path: string) => {
    if (activeTab) {
      dispatch({
        type: 'UPDATE_TAB',
        payload: {
          ...activeTab,
          selectedFolder: path
        }
      });
    }
  }, [activeTab, dispatch]);

  if (!folderStructure) {
    return <div className="p-2">Loading...</div>;
  }

  return (
    <div className="h-full overflow-y-auto">
      <FolderTreeItem
        folder={folderStructure}
        level={0}
        onToggle={handleToggle}
        onSelect={handleSelect}
        selectedPath={activeTab?.selectedFolder || null}
      />
    </div>
  );
};

export default FolderTree;
