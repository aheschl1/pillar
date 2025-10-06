import { createContext, useContext, useState, useEffect } from 'react';

export const ServerContext = createContext();

export const ServerProvider = ({ children }) => {
    const searchParams = new URLSearchParams(window.location.search);
    const [ipAddress, setIpAddress] = useState(searchParams.get('ip_address') || '127.0.0.1');
    const [httpPort, setHttpPort] = useState(searchParams.get('httpPort') || '3000');
    const [logWsPort, setLogWsPort] = useState(searchParams.get('logWsPort') || '3001');
    const [isConnected, setIsConnected] = useState(false);

    useEffect(() => {
        const newSearchParams = new URLSearchParams();
        newSearchParams.set('ip_address', ipAddress);
        newSearchParams.set('httpPort', httpPort);
        newSearchParams.set('logWsPort', logWsPort);
        const newUrl = `${window.location.pathname}?${newSearchParams.toString()}`;
        window.history.replaceState({ path: newUrl }, '', newUrl);
    }, [ipAddress, httpPort, logWsPort]);

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
