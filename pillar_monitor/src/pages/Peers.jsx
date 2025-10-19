import React, { useState, useEffect } from 'react';
import { usePeers } from '../hooks/usePeers';
import { useAddPeer } from '../hooks/useAddPeer';
import { useDiscoverPeers } from '../hooks/useDiscoverPeers';
import './Peers.css';

const AddPeerForm = ({ onPeerAdded }) => {
    const [ipAddress, setIpAddress] = useState('');
    const [port, setPort] = useState('13000'); // default port
    const [publicKey, setPublicKey] = useState('');
    const { addPeer, loading, error, success } = useAddPeer();
    const [showSuccess, setShowSuccess] = useState(false);

    useEffect(() => {
        if (success) {
            setShowSuccess(true);
            setIpAddress('');
            setPort('13000');
            setPublicKey('');
            const timer = setTimeout(() => {
                setShowSuccess(false);
            }, 3000);
            return () => clearTimeout(timer);
        }
    }, [success]);

    const handleSubmit = async (e) => {
        e.preventDefault();
        const added = await addPeer({ ip_address: ipAddress, port, public_key: publicKey });
        if (added && onPeerAdded) {
            onPeerAdded();
        }
    };

    return (
        <div className="add-peer-container">
            <form onSubmit={handleSubmit} className="add-peer-form">
                <h3>Add New Peer</h3>
                <div className="form-grid">
                    <input
                        type="text"
                        placeholder="IP Address"
                        value={ipAddress}
                        onChange={(e) => setIpAddress(e.target.value)}
                        required
                    />
                    <input
                        type="number"
                        placeholder="Port"
                        value={port}
                        onChange={(e) => setPort(e.target.value)}
                        required
                    />
                    <input
                        type="text"
                        placeholder="Public Key (optional)"
                        value={publicKey}
                        onChange={(e) => setPublicKey(e.target.value)}
                    />
                </div>
                <div className="form-footer">
                    <button type="submit" disabled={loading}>
                        {loading ? 'Adding...' : 'Add Peer'}
                    </button>
                    {showSuccess && <p className="success-message">✅ Peer added successfully</p>}
                </div>
                {error && <p className="error-message">{error}</p>}
            </form>
        </div>
    );
};


const Peers = () => {
    const { peers, loading, error, refetch } = usePeers();
    const { discoverPeers, loading: discovering, success: discoverSuccess, error: discoverError } = useDiscoverPeers();
    const [showAddPanel, setShowAddPanel] = useState(false);
    const [searchTerm, setSearchTerm] = useState('');
    const [showDiscoverSuccess, setShowDiscoverSuccess] = useState(false);

    useEffect(() => {
        if (discoverSuccess) {
            setShowDiscoverSuccess(true);
            refetch();
            const timer = setTimeout(() => {
                setShowDiscoverSuccess(false);
            }, 3000);
            return () => clearTimeout(timer);
        }
    }, [discoverSuccess, refetch]);

    const handleDiscover = async () => {
        await discoverPeers();
    };

    const peersArr = Array.isArray(peers) ? peers : [];
    const filteredPeers = peersArr.filter(peer =>
        (peer.public_key || '').toLowerCase().includes(searchTerm.toLowerCase()) ||
        (peer.ip_address || '').toLowerCase().includes(searchTerm.toLowerCase())
    );

    return (
        <div className="peers-wrapper">
            <div className="peers-header">
                <h2>Connected Peers</h2>
                <div className="header-actions">
                    <input
                        type="text"
                        placeholder="Search by public key or IP"
                        value={searchTerm}
                        onChange={(e) => setSearchTerm(e.target.value)}
                        className="search-bar"
                    />
                    <button onClick={handleDiscover} disabled={discovering} className="discover-button">
                        {discovering ? 'Discovering...' : 'Discover'}
                    </button>
                    <button
                        className={`add-peer-toggle ${showAddPanel ? 'open' : ''}`}
                        onClick={() => setShowAddPanel(s => !s)}
                    >
                        {showAddPanel ? '− Hide Form' : '+ Add Peer'}
                    </button>
                </div>
            </div>

            <div className={`add-panel ${showAddPanel ? 'open' : ''}`}>
                <AddPeerForm onPeerAdded={() => { refetch(); setShowAddPanel(false); }} />
            </div>

            {loading && <p className="status-message">Loading peers...</p>}
            {error && <p className="error-message">{error}</p>}
            {showDiscoverSuccess && <p className="success-message">✅ Peer discovery initiated.</p>}
            {discoverError && <p className="error-message">{discoverError}</p>}

            <div className="peers-grid">
                {filteredPeers.map(peer => (
                    <div key={peer.public_key} className="peer-card">
                        <div className="peer-top">
                            <h4>Peer</h4>
                            <span className="peer-address">
                                {peer.ip_address}:{peer.port}
                            </span>
                        </div>
                        <div className="peer-body">
                            <label>Public Key</label>
                            <p className="peer-key">{peer.public_key || 'N/A'}</p>
                        </div>
                    </div>
                ))}
                {filteredPeers.length === 0 && !loading && (
                    <p className="empty-message">No peers found</p>
                )}
            </div>
        </div>
    );
};

export default Peers;
