import { useState, useEffect } from 'react';
import { useServer } from '../contexts/serverContext';
import Convert from 'ansi-to-html';

const convert = new Convert();

export const useLogs = () => {
    const { ipAddress, logWsPort, isConnected, setIsConnected } = useServer();
    const [logs, setLogs] = useState([]);

    useEffect(() => {
        let ws;

        if (isConnected) {
            ws = new WebSocket(`ws://${ipAddress}:${logWsPort}/logs`);

            ws.onopen = () => {
                console.log('Connected to log stream');
                if (isConnected) { // Check if still supposed to be connected
                    setIsConnected(true);
                    setLogs(prevLogs => [...prevLogs, {
                        type: 'status',
                        message: 'Connected to log stream'
                    }]);
                } else {
                    ws.close();
                }
            };

            ws.onmessage = (event) => {
                const html = convert.toHtml(event.data);
                setLogs(prevLogs => [...prevLogs, {
                    type: 'log',
                    message: html
                }]);
            };

            ws.onclose = () => {
                console.log('Disconnected from log stream');
                setIsConnected(false);
                setLogs(prevLogs => [...prevLogs, {
                    type: 'status',
                    message: 'Disconnected from log stream'
                }]);
            };

            ws.onerror = (error) => {
                console.error('Log stream error:', error);
                setIsConnected(false);
                setLogs(prevLogs => [...prevLogs, {
                    type: 'error',
                    message: 'Log stream error'
                }]);
            };
        }

        return () => {
            if (ws) {
                ws.close();
            }
        };
    }, [ipAddress, logWsPort, isConnected, setIsConnected]);

    return logs;
};
