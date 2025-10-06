import React from 'react';
import { Container, Grid, TextField, Button, AppBar, Toolbar, Typography, Box, CssBaseline, Paper } from '@mui/material';
import usePillar from './hooks/usePillar';
import PeerManager from './components/PeerManager';
import BlockExplorer from './components/BlockExplorer';
import TransactionSender from './components/TransactionSender';
import LogViewer from './components/LogViewer';
import './App.css';

function App() {
  const { ipAddress, setIpAddress, connect, isConnected, sendTransaction, lastMessage, api, logs, isLogConnected } = usePillar();

  return (
    <>
      <CssBaseline />
      <AppBar position="static">
        <Toolbar>
          <Typography variant="h6" component="div" sx={{ flexGrow: 1 }}>
            Pillar Node Monitor
          </Typography>
        </Toolbar>
      </AppBar>
      <Container maxWidth={false} sx={{ mt: 4, mb: '270px' /* Make space for the fixed footer */ }}>
        <Paper elevation={3} sx={{ p: 2, mb: 4 }}>
            <Grid container spacing={2} alignItems="center">
                <Grid item xs={12} sm={8}>
                    <TextField
                    label="Server IP Address"
                    value={ipAddress}
                    onChange={(e) => setIpAddress(e.target.value)}
                    fullWidth
                    />
                </Grid>
                <Grid item xs={12} sm={4}>
                    <Button variant="contained" color={isConnected ? "success" : "primary"} onClick={connect} fullWidth>
                        {isConnected ? 'Connected' : 'Connect to Node'}
                    </Button>
                </Grid>
            </Grid>
        </Paper>

        <Grid container spacing={4}>
          <Grid item xs={12} md={6}>
            <PeerManager api={api} />
          </Grid>
          <Grid item xs={12} md={6}>
            <TransactionSender sendTransaction={sendTransaction} lastMessage={lastMessage} />
          </Grid>
          <Grid item xs={12}>
            <BlockExplorer api={api} />
          </Grid>
        </Grid>
      </Container>
      <LogViewer logs={logs} isConnected={isLogConnected} />
    </>
  );
}

export default App;
