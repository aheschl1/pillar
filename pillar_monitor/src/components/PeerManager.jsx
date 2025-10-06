import React, { useState } from 'react';
import { Card, CardContent, Typography, TextField, Button, Grid, List, ListItem, ListItemText, CircularProgress, Box } from '@mui/material';

const PeerManager = ({ api }) => {
    const [peers, setPeers] = useState([]);
    const [loadingPeers, setLoadingPeers] = useState(false);
    const [newPeerIp, setNewPeerIp] = useState('');
    const [newPeerPort, setNewPeerPort] = useState('');
    const [newPeerPublicKey, setNewPeerPublicKey] = useState('');
    const [addingPeer, setAddingPeer] = useState(false);

    const toHexString = (byteArray) => {
        return Array.from(byteArray, byte => {
            return ('0' + (byte & 0xFF).toString(16)).slice(-2);
        }).join('');
    }

    const fetchPeers = async () => {
        setLoadingPeers(true);
        const response = await api.get('/peers');
        if (response.success) {
            setPeers(response.body);
        } else {
            alert(`Error fetching peers: ${response.error}`);
        }
        setLoadingPeers(false);
    };

    const handleAddPeer = async () => {
        setAddingPeer(true);
        const port = parseInt(newPeerPort, 10);
        if (isNaN(port)) {
            alert('Invalid port');
            setAddingPeer(false);
            return;
        }
        const body = { ip_address: newPeerIp, port };
        const path = newPeerPublicKey ? `/peer/${newPeerPublicKey}` : '/peer';
        const response = await api.post(path, body);
        if (response.success) {
            alert('Peer added successfully');
            fetchPeers();
        } else {
            alert(`Error adding peer: ${response.error}`);
        }
        setAddingPeer(false);
    };

    return (
        <Card>
            <CardContent>
                <Grid container spacing={2} justifyContent="space-between" alignItems="center">
                    <Grid item>
                        <Typography variant="h5" gutterBottom>Peer Management</Typography>
                    </Grid>
                    <Grid item>
                        <Button variant="contained" onClick={fetchPeers} disabled={loadingPeers}>
                            {loadingPeers ? <CircularProgress size={24} /> : 'Refresh Peers'}
                        </Button>
                    </Grid>
                </Grid>
                <Box sx={{ maxHeight: 300, overflow: 'auto', my: 2 }}>
                    <List>
                        {peers.map((peer, index) => (
                            <ListItem key={index}>
                                <ListItemText
                                    primary={`IP: ${peer.ip_address}:${peer.port}`}
                                    secondary={`Public Key: ${toHexString(peer.public_key)}`}
                                    secondaryTypographyProps={{ style: { wordBreak: 'break-all' } }}
                                />
                            </ListItem>
                        ))}
                    </List>
                </Box>
                <Grid container spacing={2}>
                    <Grid item xs={12}><Typography variant="h6">Add Peer</Typography></Grid>
                    <Grid item xs={12} sm={6}>
                        <TextField label="IP Address" value={newPeerIp} onChange={(e) => setNewPeerIp(e.target.value)} fullWidth />
                    </Grid>
                    <Grid item xs={12} sm={6}>
                        <TextField label="Port" value={newPeerPort} onChange={(e) => setNewPeerPort(e.target.value)} fullWidth />
                    </Grid>
                    <Grid item xs={12}>
                        <TextField label="Public Key (Optional)" value={newPeerPublicKey} onChange={(e) => setNewPeerPublicKey(e.target.value)} fullWidth />
                    </Grid>
                    <Grid item xs={12}>
                        <Button variant="contained" onClick={handleAddPeer} disabled={addingPeer} fullWidth>
                            {addingPeer ? <CircularProgress size={24} /> : 'Add Peer'}
                        </Button>
                    </Grid>
                </Grid>
            </CardContent>
        </Card>
    );
};

export default PeerManager;
