import { useState, useEffect, useCallback } from 'react';

const usePillar = () => {
  const [ipAddress, setIpAddress] = useState('127.0.0.1');
  
  // Main WS
  const [ws, setWs] = useState(null);
  const [isConnected, setIsConnected] = useState(false);
  const [lastMessage, setLastMessage] = useState(null);

  // Log WS
  const [logWs, setLogWs] = useState(null);
  const [isLogConnected, setIsLogConnected] = useState(false);
  const [logs, setLogs] = useState([]);

  const connectToLogs = useCallback(() => {
    if (logWs) {
      logWs.close();
    }
    const newLogWs = new WebSocket(`ws://${ipAddress}:3001/logs`);
    newLogWs.onopen = () => {
      console.log('Log WebSocket connected');
      setIsLogConnected(true);
      setLogs(prev => [...prev, 'Log stream connected.']);
    };
    newLogWs.onmessage = (event) => {
      setLogs(prev => [...prev, event.data]);
    };
    newLogWs.onclose = () => {
      console.log('Log WebSocket disconnected');
      setIsLogConnected(false);
      setLogWs(null);
      if (isLogConnected) { // Only show disconnected message if it was previously connected
          setLogs(prev => [...prev, 'Log stream disconnected.']);
      }
    };
    newLogWs.onerror = (error) => {
      console.error('Log WebSocket error:', error);
      setIsLogConnected(false);
      setLogWs(null);
      setLogs(prev => [...prev, 'Log stream connection error.']);
    };
    setLogWs(newLogWs);
  }, [ipAddress, logWs, isLogConnected]);


  const connect = useCallback(() => {
    // Main WS
    if (ws) {
      ws.close();
    }
    const newWs = new WebSocket(`ws://${ipAddress}:3000/ws`);
    newWs.onopen = () => {
      console.log('WebSocket connected');
      setIsConnected(true);
    };
    newWs.onmessage = (event) => {
      console.log('WebSocket message received:', event.data);
      setLastMessage(JSON.parse(event.data));
    };
    newWs.onclose = () => {
      console.log('WebSocket disconnected');
      setIsConnected(false);
      setWs(null);
    };
    newWs.onerror = (error) => {
      console.error('WebSocket error:', error);
      setIsConnected(false);
      setWs(null);
    };
    setWs(newWs);

    // Log WS
    connectToLogs();

  }, [ipAddress, ws, connectToLogs]);

  useEffect(() => {
    return () => {
      if (ws) {
        ws.close();
      }
      if (logWs) {
        logWs.close();
      }
    };
  }, [ws, logWs]);

  const sendTransaction = (transaction) => {
    if (isConnected) {
      const message = {
        type: 'TransactionPost',
        ...transaction,
      };
      ws.send(JSON.stringify(message));
    } else {
      console.error('WebSocket is not connected');
    }
  };

  const api = {
    get: async (path) => {
      try {
        const response = await fetch(`http://${ipAddress}:3000${path}`);
        return await response.json();
      } catch (error) {
        console.error(`GET ${path} failed:`, error);
        return { success: false, error: error.message, body: null };
      }
    },
    post: async (path, body) => {
      try {
        const response = await fetch(`http://${ipAddress}:3000${path}`, {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify(body),
        });
        return await response.json();
      } catch (error) {
        console.error(`POST ${path} failed:`, error);
        return { success: false, error: error.message, body: null };
      }
    },
  };

  return {
    ipAddress,
    setIpAddress,
    connect,
    isConnected,
    sendTransaction,
    lastMessage,
    api,
    logs,
    isLogConnected
  };
};

export default usePillar;
