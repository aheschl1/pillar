import React from 'react';
import { useServer } from '../../contexts/serverContext';
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

    // a form, so Enter in any field connects
    const handleConnect = (event) => {
        event.preventDefault();
        const form = event.currentTarget;
        setIpAddress(form.querySelector('#ipAddressInput').value.trim());
        setHttpPort(form.querySelector('#httpPortInput').value.trim());
        setLogWsPort(form.querySelector('#logWsPortInput').value.trim());
        setIsConnected(true);
    };

    const handleDisconnect = () => {
        setIsConnected(false);
    };

    const httpStatusColor = isConnected && nodeData && !nodeError ? 'var(--success)' : 'var(--danger)';
    const wsStatusColor = isConnected ? 'var(--success)' : 'var(--danger)'; // This assumes ws connects if isConnected is true

    return (
        <div className="server-bar">
            <form className="server-bar-controls" onSubmit={handleConnect}>
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
                        <button type="button" onClick={handleDisconnect} className="connect-button disconnect">Disconnect</button>
                    ) : (
                        <button type="submit" className="connect-button">Connect</button>
                    )}
                </div>
            </form>
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
