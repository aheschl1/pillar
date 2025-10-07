import React, { useState, useEffect } from 'react';
import { usePeers } from '../hooks/usePeers';
import { useAddPeer } from '../hooks/useAddPeer';
import './Peers.css';

const AddPeerForm = ({ onPeerAdded }) => {
    const [ipAddress, setIpAddress] = useState('');
    const [port, setPort] = useState('');
    const [publicKey, setPublicKey] = useState('');
    const { addPeer, loading, error, success } = useAddPeer();
    const [showSuccess, setShowSuccess] = useState(false);

    useEffect(() => {
        if (success) {
            setShowSuccess(true);
            setIpAddress('');
            setPort('');
            setPublicKey('');
            const timer = setTimeout(() => {
                setShowSuccess(false);
            }, 3000); // Hide after 3 seconds
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
        <div className="add-peer-card">
            <form onSubmit={handleSubmit} className="add-peer-form">
                <h3>Add a Peer</h3>
                <div className="form-group">
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
                <div className="form-actions">
                    <button type="submit" disabled={loading}>
                        {loading ? 'Adding...' : 'Add Peer'}
                    </button>
                    {showSuccess && <p className="success-message">Peer added successfully</p>}
                </div>
                {error && <p className="error-message">{error}</p>}
            </form>
        </div>
    );
};


const Peers = () => {
    const { peers, loading, error, refetch } = usePeers();
    const [showAddPanel, setShowAddPanel] = useState(false);
    const [searchTerm, setSearchTerm] = useState('');

    // defensive: ensure peers is an array
    const peersArr = Array.isArray(peers) ? peers : [];
    const filteredPeers = peersArr.filter(peer =>
        (peer.public_key || '').toLowerCase().includes(searchTerm.toLowerCase()) ||
        (peer.ip_address || '').toLowerCase().includes(searchTerm.toLowerCase())
    );

    const toggleAddPanel = () => setShowAddPanel(s => !s);

    return (
        <div className="peers-container">
            <div className="peers-header">
                <h2>Peers</h2>
                <div className="header-controls">
                    <input
                        type="text"
                        placeholder="Search by public key or IP"
                        value={searchTerm}
                        onChange={(e) => setSearchTerm(e.target.value)}
                        className="search-bar"
                    />
                    <button
                        className={`add-toggle-inline ${showAddPanel ? 'open' : ''}`}
                        onClick={toggleAddPanel}
                        aria-expanded={showAddPanel}
                        aria-label={showAddPanel ? 'Hide Add Peer' : 'Show Add Peer'}
                    >
                        <span className="arrow">▾</span>
                        <span className="label">Add</span>
                    </button>
                </div>
            </div>

            {/* Slide-down Add Peer panel. Hidden by default. */}
            <div className={`add-panel ${showAddPanel ? 'open' : ''}`}>
                <AddPeerForm onPeerAdded={() => { refetch(); setShowAddPanel(false); }} />
            </div>
            {loading && <p>Loading peers...</p>}
            {error && <p className="error-message">{error}</p>}
            <div className="peers-grid">
                {filteredPeers.map(peer => (
                    <div key={peer.public_key} className="peer-card">
                        <div className="peer-info">
                            <strong>Public Key:</strong>
                            <span className="peer-pk">{peer.public_key}</span>
                        </div>
                        <div className="peer-info">
                            <strong>Address:</strong>
                            <span>{peer.ip_address}:{peer.port}</span>
                        </div>
                    </div>
                ))}
            </div>
        </div>
    );
};

export default Peers;
