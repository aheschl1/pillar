import React, { useState } from 'react';
import { usePeers } from '../hooks/usePeers';
import './Peers.css';

const Peers = () => {
    const { peers, loading, error } = usePeers();
    const [searchTerm, setSearchTerm] = useState('');

    const filteredPeers = peers.filter(peer =>
        peer.public_key.toLowerCase().includes(searchTerm.toLowerCase()) ||
        peer.ip_address.toLowerCase().includes(searchTerm.toLowerCase())
    );

    return (
        <div className="peers-container">
            <div className="peers-header">
                <h2>Peers</h2>
                <input
                    type="text"
                    placeholder="Search by public key or IP"
                    value={searchTerm}
                    onChange={(e) => setSearchTerm(e.target.value)}
                    className="search-bar"
                />
            </div>
            {loading && <p>Loading peers...</p>}
            {error && <p className="error-message">Error: {error}</p>}
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
