import { useState } from 'react';

const FILTERS = ['全部', '连接', '命令', '文件', 'AI'] as const;

/** Windows home-only activity rail. Activity persistence is added when audit events expose a feed API. */
export default function HomeActivityPanel() {
  const [filter, setFilter] = useState<(typeof FILTERS)[number]>('全部');

  return (
    <aside className="home-activity" aria-label="活动记录">
      <div className="home-activity-heading">活动记录</div>
      <div className="activity-filters">
        {FILTERS.map((item) => (
          <button key={item} className={filter === item ? 'active' : ''} onClick={() => setFilter(item)}>{item}</button>
        ))}
      </div>
      <div className="activity-empty">
        <span>▤</span>
        <p>暂无活动</p>
      </div>
    </aside>
  );
}
