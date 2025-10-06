import React, { useState, useEffect } from 'react';
import { Card, CardContent, Typography, TextField, Button, Grid, CircularProgress, Paper } from '@mui/material';

const TransactionSender = ({ sendTransaction, lastMessage }) => {
    const [to, setTo] = useState('');
    const [amount, setAmount] = useState('');
    const [sending, setSending] = useState(false);

    const handleSend = () => {
        setSending(true);
        const transaction = {
            to,
            amount: parseInt(amount, 10),
            asset: '0'.repeat(64), // Placeholder for asset
        };
        sendTransaction(transaction);
        // Note: We don't setSending(false) here immediately.
        // We could wait for a response from the websocket if we want.
        // For now, we'll just let the user know it's sent.
        setTimeout(() => setSending(false), 1000); // Simulate sending
    };

    return (
        <Card>
            <CardContent>
                <Typography variant="h5" gutterBottom>Send Transaction (WS)</Typography>
                <Grid container spacing={2}>
                    <Grid item xs={12}>
                        <TextField label="Recipient Address" value={to} onChange={(e) => setTo(e.target.value)} fullWidth />
                    </Grid>
                    <Grid item xs={12}>
                        <TextField label="Amount" value={amount} onChange={(e) => setAmount(e.target.value)} type="number" fullWidth />
                    </Grid>
                    <Grid item xs={12}>
                        <Button variant="contained" onClick={handleSend} disabled={sending}>
                            {sending ? <CircularProgress size={24} /> : 'Send'}
                        </Button>
                    </Grid>
                    {lastMessage && (
                        <Grid item xs={12}>
                             <Paper elevation={3} style={{ padding: '15px', marginTop: '15px', overflowX: 'auto' }}>
                                <Typography>Last WS Message:</Typography>
                                <pre>{JSON.stringify(lastMessage, null, 2)}</pre>
                            </Paper>
                        </Grid>
                    )}
                </Grid>
            </CardContent>
        </Card>
    );
};

export default TransactionSender;
