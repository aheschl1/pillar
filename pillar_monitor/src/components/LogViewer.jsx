import React, { useEffect, useRef } from 'react';
import { Paper, Typography, Box } from '@mui/material';
import Convert from 'ansi-to-html';

const convert = new Convert();

const LogViewer = ({ logs, isConnected }) => {
    const scrollRef = useRef(null);

    useEffect(() => {
        if (scrollRef.current) {
            scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
        }
    }, [logs]);

    return (
        <Paper elevation={5} sx={{
            position: 'fixed',
            bottom: 0,
            left: 0,
            right: 0,
            height: '250px',
            backgroundColor: '#1e1e1e',
            color: '#d4d4d4',
            fontFamily: 'monospace',
            p: 2,
            borderTop: '1px solid #333'
        }}>
            <Typography variant="h6" sx={{ mb: 1, color: '#569cd6' }}>Node Logs</Typography>
            <Box
                ref={scrollRef}
                sx={{
                    height: 'calc(100% - 40px)',
                    overflowY: 'auto',
                    '&::-webkit-scrollbar': {
                        width: '0.4em'
                    },
                    '&::-webkit-scrollbar-track': {
                        boxShadow: 'inset 0 0 6px rgba(0,0,0,0.00)',
                        webkitBoxShadow: 'inset 0 0 6px rgba(0,0,0,0.00)'
                    },
                    '&::-webkit-scrollbar-thumb': {
                        backgroundColor: 'rgba(255,255,255,.1)',
                        outline: '1px solid slategrey'
                    }
                }}
            >
                {logs.map((log, index) => (
                    <Typography
                        key={index}
                        component="div"
                        sx={{ whiteSpace: 'pre-wrap', wordBreak: 'break-all' }}
                        dangerouslySetInnerHTML={{ __html: convert.toHtml(log) }}
                    />
                ))}
                {!isConnected && <Typography>Disconnected from log stream.</Typography>}
            </Box>
        </Paper>
    );
};

export default LogViewer;
