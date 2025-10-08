import React, { useContext, useState, useEffect } from 'react';
import { ServerContext, useServer } from '../../contexts/serverContext';
import { useNodeData } from '../../hooks/useNodeData';
import { useHttp } from '../../hooks/useHttp';
import './ServerBar.css';

const ServerBar = () => {
    const { 
        ipAddress, setIpAddress, 
        httpPort, setHttpPort, 
        logWsPort, setLogWsPort,
        isConnected, setIsConnected
    } = useServer();

    const { nodeData, error: nodeError } = useNodeData();
    const { post } = useHttp();

    const handleConnectToggle = (event) => {
        if (isConnected) {
            setIsConnected(false);
        } else {
            // On connect, we read the values directly from the form fields
            const form = event.currentTarget.closest('.server-bar-controls');
            const newIp = form.querySelector('#ipAddressInput').value;
            const newHttp = form.querySelector('#httpPortInput').value;
            const newLogWs = form.querySelector('#logWsPortInput').value;

            setIpAddress(newIp);
            setHttpPort(newHttp);
            setLogWsPort(newLogWs);
            setIsConnected(true);
        }
    };

    const handleDisconnect = () => {
        setIsConnected(false);
    };

    const httpStatusColor = isConnected && nodeData && !nodeError ? 'green' : 'red';
    const wsStatusColor = isConnected ? 'green' : 'red'; // This assumes ws connects if isConnected is true

    return (
        <div className="server-bar">
            <div className="server-bar-controls">
                <div className="server-bar-item">
                    <label>IP Address:</label>
                    <input 
                        id="ipAddressInput"
                        type="text" 
                        defaultValue={ipAddress}
                        disabled={isConnected} 
                    />
                </div>
                <div className="server-bar-item">
                    <span className="status-dot" style={{ backgroundColor: httpStatusColor }}></span>
                    <label>HTTP Port:</label>
                    <input 
                        id="httpPortInput"
                        type="text" 
                        defaultValue={httpPort}
                        disabled={isConnected} 
                    />
                </div>
                <div className="server-bar-item">
                    <span className="status-dot" style={{ backgroundColor: wsStatusColor }}></span>
                    <label>Log WS Port:</label>
                    <input 
                        id="logWsPortInput"
                        type="text" 
                        defaultValue={logWsPort}
                        disabled={isConnected} 
                    />
                </div>
                <div className="server-bar-item">
                    {isConnected ? (
                        <button onClick={handleDisconnect} className="connect-button disconnect">Disconnect</button>
                    ) : (
                        <button onClick={handleConnectToggle} className="connect-button">Connect</button>
                    )}
                </div>
            </div>
            <div className="server-bar-info">
                {isConnected && nodeData && (
                    <div className="server-bar-item">
                        <label>Public Key:</label>
                        <span className="public-key">{nodeData.public_key}</span>
                        <div className="server-bar-item">
                            <label style={{marginLeft: '8px'}}>Node State:</label>
                            <span
                                className="public-key clickable"
                                title="Click to initialize chain"
                                onClick={async () => {
                                    const ok = window.confirm('This will initialize the chain on the node. Are you sure?');
                                    if (!ok) return;
                                    try {
                                        const res = await post(`/init`, {});
                                        if (res && res.success) {
                                            window.alert('Init triggered successfully');
                                        } else {
                                            window.alert('Init failed: ' + (res.error || 'unknown'));
                                        }
                                    } catch (err) {
                                        window.alert('Init failed: ' + err.message);
                                    }
                                }}
                            >
                                {nodeData.state}
                            </span>
                        </div>
                    </div>
                )}
            </div>
        </div>
    );
};

export default ServerBar;
