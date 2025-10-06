import React, { useContext, useState, useEffect } from 'react';
import { ServerContext, useServer } from '../../contexts/serverContext';
import { useNodeData } from '../../hooks/useNodeData';
import './ServerBar.css';

const ServerBar = () => {
    const { 
        ipAddress, setIpAddress, 
        httpPort, setHttpPort, 
        logWsPort, setLogWsPort,
        isConnected, setIsConnected
    } = useServer();

    const { nodeData, error: nodeError } = useNodeData();

    const [localIp, setLocalIp] = useState(ipAddress);
    const [localHttp, setLocalHttp] = useState(httpPort);
    const [localLogWs, setLocalLogWs] = useState(logWsPort);

    useEffect(() => {
        setLocalIp(ipAddress);
        setLocalHttp(httpPort);
        setLocalLogWs(logWsPort);
    }, [ipAddress, httpPort, logWsPort]);

    const handleConnectToggle = () => {
        if (isConnected) {
            setIsConnected(false);
        } else {
            setIpAddress(localIp);
            setHttpPort(localHttp);
            setLogWsPort(localLogWs);
            setIsConnected(true);
        }
    };

    const httpStatusColor = isConnected && nodeData && !nodeError ? 'green' : 'red';
    const wsStatusColor = isConnected ? 'green' : 'red'; // This assumes ws connects if isConnected is true

    return (
        <div className="server-bar">
            <div className="server-bar-controls">
                <div className="server-bar-item">
                    <label>IP Address:</label>
                    <input 
                        type="text" 
                        value={localIp}
                        onChange={(e) => setLocalIp(e.target.value)}
                        disabled={isConnected} 
                    />
                </div>
                <div className="server-bar-item">
                    <span className="status-dot" style={{ backgroundColor: httpStatusColor }}></span>
                    <label>HTTP Port:</label>
                    <input 
                        type="text" 
                        value={localHttp}
                        onChange={(e) => setLocalHttp(e.target.value)}
                        disabled={isConnected} 
                    />
                </div>
                <div className="server-bar-item">
                    <span className="status-dot" style={{ backgroundColor: wsStatusColor }}></span>
                    <label>Log WS Port:</label>
                    <input 
                        type="text" 
                        value={localLogWs}
                        onChange={(e) => setLocalLogWs(e.target.value)}
                        disabled={isConnected} 
                    />
                </div>
                <div className="server-bar-item">
                    <label className="switch">
                        <input type="checkbox" checked={isConnected} onChange={handleConnectToggle} />
                        <span className="slider round"></span>
                    </label>
                    <span>{isConnected ? 'Connected' : 'Disconnected'}</span>
                </div>
            </div>
            <div className="server-bar-info">
                {isConnected && nodeData && (
                    <div className="server-bar-item">
                        <label>Public Key:</label>
                        <span className="public-key">{nodeData.public_key}</span>
                    </div>
                )}
            </div>
        </div>
    );
};

export default ServerBar;
