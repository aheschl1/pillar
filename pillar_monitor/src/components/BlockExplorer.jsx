import React, { useState } from 'react';
import { Card, CardContent, Typography, TextField, Button, Grid, CircularProgress, Box, Paper } from '@mui/material';

const BlockExplorer = ({ api }) => {
    const [blockHash, setBlockHash] = useState('');
    const [block, setBlock] = useState(null);
    const [loadingBlock, setLoadingBlock] = useState(false);

    const [minDepth, setMinDepth] = useState('');
    const [maxDepth, setMaxDepth] = useState('');
    const [limit, setLimit] = useState('');
    const [blockList, setBlockList] = useState([]);
    const [loadingBlockList, setLoadingBlockList] = useState(false);

    const handleGetBlock = async () => {
        if (!blockHash) return;
        setLoadingBlock(true);
        const response = await api.get(`/block/${blockHash}`);
        if (response.success) {
            setBlock(response.body);
        } else {
            alert(`Error fetching block: ${response.error}`);
            setBlock(null);
        }
        setLoadingBlock(false);
    };

    const handleListBlocks = async () => {
        setLoadingBlockList(true);
        let query = `?`;
        if (minDepth) query += `min_depth=${minDepth}&`;
        if (maxDepth) query += `max_depth=${maxDepth}&`;
        if (limit) query += `limit=${limit}&`;
        
        const response = await api.get(`/blocks${query}`);
        if (response.success) {
            setBlockList(response.body);
        } else {
            alert(`Error listing blocks: ${response.error}`);
        }
        setLoadingBlockList(false);
    };

    return (
        <Card>
            <CardContent>
                <Typography variant="h5" gutterBottom>Block Explorer</Typography>
                <Grid container spacing={3}>
                    {/* Get Block by Hash */}
                    <Grid item xs={12}>
                        <Typography variant="h6">Get Block by Hash</Typography>
                        <TextField
                            label="Block Hash"
                            value={blockHash}
                            onChange={(e) => setBlockHash(e.target.value)}
                            fullWidth
                            margin="normal"
                        />
                        <Button variant="contained" onClick={handleGetBlock} disabled={loadingBlock}>
                            {loadingBlock ? <CircularProgress size={24} /> : 'Get Block'}
                        </Button>
                        {block && (
                            <Paper elevation={3} style={{ padding: '15px', marginTop: '15px', overflowX: 'auto' }}>
                                <pre>{JSON.stringify(block, null, 2)}</pre>
                            </Paper>
                        )}
                    </Grid>

                    {/* List Blocks */}
                    <Grid item xs={12}>
                        <Typography variant="h6">List Blocks</Typography>
                        <Grid container spacing={2}>
                            <Grid item xs={4}>
                                <TextField label="Min Depth" value={minDepth} onChange={(e) => setMinDepth(e.target.value)} fullWidth />
                            </Grid>
                            <Grid item xs={4}>
                                <TextField label="Max Depth" value={maxDepth} onChange={(e) => setMaxDepth(e.target.value)} fullWidth />
                            </Grid>
                            <Grid item xs={4}>
                                <TextField label="Limit" value={limit} onChange={(e) => setLimit(e.target.value)} fullWidth />
                            </Grid>
                        </Grid>
                        <Button variant="contained" onClick={handleListBlocks} disabled={loadingBlockList} style={{ marginTop: '10px' }}>
                            {loadingBlockList ? <CircularProgress size={24} /> : 'List Blocks'}
                        </Button>
                        {blockList.length > 0 && (
                             <Paper elevation={3} style={{ padding: '15px', marginTop: '15px', maxHeight: '300px', overflowY: 'auto' }}>
                                {blockList.map((hash, index) => (
                                    <Typography key={index} style={{ wordBreak: 'break-all' }}>{hash}</Typography>
                                ))}
                            </Paper>
                        )}
                    </Grid>
                </Grid>
            </CardContent>
        </Card>
    );
};

export default BlockExplorer;
