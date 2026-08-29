import { useEffect, useMemo, useState } from 'react';
import type { ToolCommandMeta } from '../../lib/commandMeta';

export interface PaletteCommand {
  id: string;
  title: string;
  group: string;
  keywords?: string;
  run: () => void;
}

interface Props {
  open: boolean;
  onClose: () => void;
  commands: PaletteCommand[];
  toolCommands: Array<{ meta: ToolCommandMeta; run: () => void }>;
}

/** ⌘K palette. App commands and structured tool commands share one metadata list. */
export default function CommandPalette({ open, onClose, commands, toolCommands }: Props) {
  const [query, setQuery] = useState('');
  const [index, setIndex] = useState(0);

  const items = useMemo(() => {
    const all: PaletteCommand[] = [
      ...commands,
      ...toolCommands.map(({ meta, run }) => ({
        id: meta.id,
        title: meta.title,
        group: `工具 · ${meta.group}`,
        keywords: meta.keywords,
        run,
      })),
    ];
    const needle = query.trim().toLowerCase();
    if (!needle) return all;
    return all.filter((item) =>
      item.title.toLowerCase().includes(needle)
      || item.group.toLowerCase().includes(needle)
      || (item.keywords ?? '').toLowerCase().includes(needle));
  }, [commands, toolCommands, query]);

  useEffect(() => {
    if (open) { setQuery(''); setIndex(0); }
  }, [open]);

  if (!open) return null;

  const execute = (item?: PaletteCommand) => {
    if (!item) return;
    onClose();
    item.run();
  };

  return (
    <div className="modal-backdrop palette-backdrop" onClick={onClose}>
      <div className="modal palette" onClick={(event) => event.stopPropagation()}>
        <div className="palette-search"><span>⌕</span><input
          autoFocus
          className="palette-input"
          value={query}
          placeholder="搜索命令、工具、服务器…"
          onChange={(event) => { setQuery(event.target.value); setIndex(0); }}
          onKeyDown={(event) => {
            if (event.key === 'ArrowDown') { event.preventDefault(); setIndex((value) => Math.min(value + 1, items.length - 1)); }
            if (event.key === 'ArrowUp') { event.preventDefault(); setIndex((value) => Math.max(value - 1, 0)); }
            if (event.key === 'Enter') { event.preventDefault(); execute(items[index]); }
            if (event.key === 'Escape') onClose();
          }}
        /><kbd>Ctrl K</kbd></div>
        <div className="palette-list">
          {items.length === 0 && <div className="sidebar-empty">没有匹配的命令</div>}
          {items.map((item, position) => (
            <button
              key={item.id}
              className={`palette-item ${position === index ? 'active' : ''}`}
              onMouseEnter={() => setIndex(position)}
              onClick={() => execute(item)}
            >
              <span>{item.title}</span>
              <small>{item.group}</small>
            </button>
          ))}
        </div>
        <footer className="palette-footer"><span>↑↓ 选择</span><span>Enter 执行</span><span>Esc 关闭</span></footer>
      </div>
    </div>
  );
}
