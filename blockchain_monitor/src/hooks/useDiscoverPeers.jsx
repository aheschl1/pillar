import { useState } from 'react';
import { useHttp } from './useHttp';

export const useDiscoverPeers = () => {
    const { post, loading, error } = useHttp();
    const [success, setSuccess] = useState(false);

    const discoverPeers = async () => {
        setSuccess(false);
        const response = await post('/peer/discover', {});

        if (response && response.success) {
            setSuccess(true);
            return true;
        }
        return false;
    };

    return { discoverPeers, loading, error, success };
};
