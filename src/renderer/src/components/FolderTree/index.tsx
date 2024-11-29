import React, { useState, useEffect, useCallback } from 'react';
import { ArrowRight, ArrowDown, Folder } from '@phosphor-icons/react';
import { useTheme } from '@renderer/context/ThemeContext';
import { useTabs } from '@renderer/context/TabContext';

interface FolderNode {
  name: string;
  path: string;
  children: FolderNode[];
  isExpanded?: boolean;
}

const FolderTreeItem: React.FC<{
  folder: FolderNode;
  level: number;
  onToggle: (path: string) => void;
  onSelect: (path: string) => void;
  selectedPath: string | null;
}> = ({ folder, level, onToggle, onSelect, selectedPath }) => {
  const { isDarkMode } = useTheme();
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
          flex items-center px-2 py-1 cursor-pointer
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
          {folder.children.length > 0 && (
            folder.isExpanded ? (
              <ArrowDown className="w-4 h-4" />
            ) : (
              <ArrowRight className="w-4 h-4" />
            )
          )}
        </div>
        <Folder className="w-4 h-4 mx-2" />
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
          const childStructure = await buildFolderStructure(dir.path);
          return childStructure;
        })
      );

      return {
        name: path.split('/').pop() || path,
        path: path,
        children: children.filter((child): child is FolderNode => child !== null),
        isExpanded: expandedFolders.has(path)
      };
    } catch (error) {
      console.error('Error building folder structure:', error);
      return null;
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
    setExpandedFolders(prev => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }, []);

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