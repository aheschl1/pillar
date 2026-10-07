import { createContext, useContext, useState, useEffect } from 'react';

export const ServerContext = createContext();

// The last node connected to, so a reload reconnects to it.
const STORAGE_KEY = 'pillar-monitor-server';
const loadSaved = () => {
    try { return JSON.parse(localStorage.getItem(STORAGE_KEY)) || {}; } catch { return {}; }
};
const save = (value) => {
    try { localStorage.setItem(STORAGE_KEY, JSON.stringify(value)); } catch { /* storage unavailable */ }
};

export const ServerProvider = ({ children }) => {
    const searchParams = new URLSearchParams(window.location.search);
    const saved = loadSaved();
    // a node address in the URL wins over the saved one, and connects straight away
    const fromUrl = searchParams.has('ip_address');
    const [ipAddress, setIpAddress] = useState(searchParams.get('ip_address') || saved.ipAddress || '127.0.0.1');
    const [httpPort, setHttpPort] = useState(searchParams.get('httpPort') || saved.httpPort || '3000');
    const [logWsPort, setLogWsPort] = useState(searchParams.get('logWsPort') || saved.logWsPort || '3001');
    const [isConnected, setIsConnected] = useState(fromUrl || saved.connected === true);

    useEffect(() => {
        save({ ipAddress, httpPort, logWsPort, connected: isConnected });
        // keep the node in the URL (so it can be shared) without dropping a page's own params
        const params = new URLSearchParams(window.location.search);
        params.set('ip_address', ipAddress);
        params.set('httpPort', httpPort);
        params.set('logWsPort', logWsPort);
        window.history.replaceState(window.history.state, '', `${window.location.pathname}?${params}`);
    }, [ipAddress, httpPort, logWsPort, isConnected]);

    return (
        <ServerContext.Provider value={{ 
            ipAddress, 
            setIpAddress, 
            httpPort, 
            setHttpPort, 
            logWsPort, 
            setLogWsPort,
            isConnected,
            setIsConnected
        }}>
            {children}
        </ServerContext.Provider>
    );
};

export const useServer = () => {
    const context = useContext(ServerContext);
    if (!context) {
        throw new Error('useServer must be used within a ServerProvider');
    }
    return context;
}
