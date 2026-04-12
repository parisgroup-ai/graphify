import { api } from '@/lib/api';
import { UserService } from './services/user';

const service = new UserService();
api.get('/users');
api.get('/health');
