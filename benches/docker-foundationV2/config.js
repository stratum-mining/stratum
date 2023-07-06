/*
 *
 * Example (Main)
 *
 */

// Main Configuration
////////////////////////////////////////////////////////////////////////////////

// Miscellaneous Configuration
const config = {};
config.identifier = '';
config.language = 'english';

// Logger Configuration
config.logger = {};
config.logger.logColors = true;
config.logger.logLevel = 'log';

// Database Configuration (SQL)
config.client = {};
config.client.tls = false;

// Master Database
config.client.master = {};
config.client.master.host = '127.0.0.1';
config.client.master.port = 5432;
config.client.master.username = 'Prisca';
config.client.master.password = '12345';
config.client.master.database = 'foundation';

// Worker Database
config.client.worker = {};
config.client.worker.host = '127.0.0.1';
config.client.worker.port = 5432;
config.client.worker.username = 'Prisca';
config.client.worker.password = '123456';
config.client.worker.database = 'foundation';

// Clustering Configuration
config.clustering = {};
config.clustering.enabled = true;
config.clustering.forks = 'auto';

// TLS Configuration
config.tls = {};
config.tls.ca = '';
config.tls.key = '';
config.tls.cert = '';

// Server Configuration
config.server = {};
config.server.enabled = true;
config.server.host = '127.0.0.1';
config.server.port = 3001;
config.server.tls = false;

// Cache Configuration
config.server.cache = {};
config.server.cache.enabled = true;
config.server.cache.timing = '1 minute';

// Limiter Configuration
config.server.limiter = {};
config.server.limiter.enabled = true;
config.server.limiter.window = 900000; // ms
config.server.limiter.maximum = 100;

// Export Configuration
module.exports = config;