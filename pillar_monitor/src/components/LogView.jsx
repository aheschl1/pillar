import React, { useState, useEffect, useRef } from 'react';
import { useLogs } from '../hooks/useLogs';
import './LogView.css';

// The node's log stream; `open` and `onToggle` come from App, which sizes the panel.
const LogView = ({ open = true, onToggle }) => {
    const logs = useLogs();
    const logContainerRef = useRef(null);
    const [isFullScreen, setIsFullScreen] = useState(false);

    useEffect(() => {
        if (logContainerRef.current) {
            logContainerRef.current.scrollTop = logContainerRef.current.scrollHeight;
        }
    }, [logs, open]);

    return (
        <div className={`log-view ${isFullScreen ? 'fullscreen' : ''}`}>
            <div className="log-view-bar">
                <span>Node logs</span>
                <span className="log-view-actions">
                    {open && (
                        <button type="button" className="log-button" onClick={() => setIsFullScreen((f) => !f)}>
                            {isFullScreen ? 'Exit fullscreen' : 'Fullscreen'}
                        </button>
                    )}
                    {!isFullScreen && (
                        <button type="button" className="log-button" onClick={onToggle}>{open ? 'Hide' : 'Show'}</button>
                    )}
                </span>
            </div>
            {open && (
                <div className="log-view-container" ref={logContainerRef}>
                    {logs.map((log, index) => (
                        <div
                            key={index}
                            className={`log-entry log-${log.type}`}
                            dangerouslySetInnerHTML={{ __html: log.message }}
                        />
                    ))}
                </div>
            )}
        </div>
    );
};

export default LogView;
