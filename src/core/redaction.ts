const SENSITIVE_ASSIGNMENT = /(["']?(?:_authToken|authToken|password|passwd|secret|accessToken|refreshToken|apiKey)["']?\s*[=:]\s*["']?)([^\s,"']+)/gi;
const URL_USER_INFO = /(https?:\/\/)([^/@\s]+)@/gi;
const SENSITIVE_QUERY = /([?&](?:token|access_token|auth|password|secret|key)=)([^&\s]+)/gi;
const BEARER_TOKEN = /(authorization\s*[=:]\s*["']?bearer\s+)([^\s,"']+)/gi;

export function redactSensitiveText(content: string): string {
  return content
    .replace(URL_USER_INFO, "$1***@")
    .replace(SENSITIVE_QUERY, "$1***")
    .replace(SENSITIVE_ASSIGNMENT, "$1***")
    .replace(BEARER_TOKEN, "$1***");
}
