import { useState, useEffect, useCallback } from 'react';
import { useServer } from '../contexts/serverContext';

export const useHttp = (endpoint, autopoll = true) => {
    const { ipAddress, httpPort, isConnected } = useServer();
    const [data, setData] = useState(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState(null);

    const fetchData = useCallback(async () => {
        if (!isConnected || !endpoint) {
            setData(null);
            return;
        }

        setLoading(true);
        setError(null);

        try {
            const response = await fetch(`http://${ipAddress}:${httpPort}${endpoint}`);
            if (!response.ok) {
                throw new Error(`HTTP error! status: ${response.status}`);
            }
            const result = await response.json();
            if (result.success && result.body) {
                setData(result.body);
            } else {
                throw new Error(result.error || `Failed to fetch from ${endpoint}`);
            }
        } catch (e) {
            setError(e.message);
            setData(null);
        } finally {
            setLoading(false);
        }
    }, [ipAddress, httpPort, isConnected, endpoint]);

    useEffect(() => {
        fetchData();

        let intervalId;
        if (autopoll) {
            intervalId = setInterval(fetchData, 5000); // Poll every 5 seconds
        }

        return () => { if (intervalId) clearInterval(intervalId); };

    }, [fetchData, autopoll]);

    const post = useCallback(async (url, body) => {
        if (!isConnected) {
            throw new Error("Not connected to server");
        }
        const response = await fetch(`http://${ipAddress}:${httpPort}${url}`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify(body),
        });
        if (!response.ok) {
            throw new Error(`HTTP error! status: ${response.status}`);
        }
        return await response.json();
    }, [ipAddress, httpPort, isConnected]);


    return { data, loading, error, refetch: fetchData, post };
};
