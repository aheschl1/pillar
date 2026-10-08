import { useRef, useState } from 'react';
import { useServer } from '../contexts/serverContext';

export const useWs = (path = '/ws') => {
    const { ipAddress, httpPort } = useServer();
    const wsRef = useRef(null);
    const [connected, setConnected] = useState(false);
    const [messages, setMessages] = useState([]);

    const connect = () => {
        return new Promise((resolve, reject) => {
            if (!ipAddress || !httpPort) return reject(new Error('Missing server info'));
            try {
                const scheme = window.location.protocol === 'https:' ? 'wss' : 'ws';
                const url = `${scheme}://${ipAddress}:${httpPort}${path}`;
                const ws = new WebSocket(url);
                wsRef.current = ws;

                ws.onopen = () => {
                    setConnected(true);
                    resolve(true);
                };
                ws.onclose = () => setConnected(false);
                ws.onerror = (ev) => {
                    setConnected(false);
                };
                ws.onmessage = (evt) => {
                    try {
                        const data = JSON.parse(evt.data);
                        setMessages(m => [...m, data]);
                    } catch (e) {
                        setMessages(m => [...m, evt.data]);
                    }
                };
            } catch (e) {
                setConnected(false);
                reject(e);
            }
        });
    };

    const disconnect = () => {
        try {
            if (wsRef.current) {
                wsRef.current.close();
                wsRef.current = null;
            }
        } catch (e) {}
        setConnected(false);
    };

    const send = (obj) => {
        if (wsRef.current && wsRef.current.readyState === WebSocket.OPEN) {
            wsRef.current.send(JSON.stringify(obj));
            return true;
        }
        return false;
    };

    return { connected, messages, connect, disconnect, send };
};

export default useWs;
