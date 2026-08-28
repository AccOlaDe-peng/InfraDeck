import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './app/App';
import { isMac } from './lib/platform';
import './styles.css';

// macOS 交通灯与顶栏融合时，顶栏需要为红绿灯预留左侧空间
document.documentElement.classList.toggle('is-mac', isMac);

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
