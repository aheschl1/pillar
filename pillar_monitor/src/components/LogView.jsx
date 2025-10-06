import React, { useState, useEffect, useRef } from 'react';
import { useLogs } from '../hooks/useLogs';
import './LogView.css';

const LogView = () => {
    const logs = useLogs();
    const logContainerRef = useRef(null);
    const [isFullScreen, setIsFullScreen] = useState(false);

    useEffect(() => {
        if (logContainerRef.current) {
            logContainerRef.current.scrollTop = logContainerRef.current.scrollHeight;
        }
    }, [logs]);

    const toggleFullScreen = () => {
        setIsFullScreen(!isFullScreen);
    };

    const containerClasses = `log-view-container ${isFullScreen ? 'fullscreen' : ''}`;

    return (
        <div className={containerClasses} ref={logContainerRef}>
            <button className="fullscreen-button" onClick={toggleFullScreen}>
                {isFullScreen ? 'Exit Fullscreen' : 'Fullscreen'}
            </button>
            {logs.map((log, index) => (
                <div
                    key={index}
                    className={`log-entry log-${log.type}`}
                    dangerouslySetInnerHTML={{ __html: log.message }}
                />
            ))}
        </div>
    );
};

export default LogView;
