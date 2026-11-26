import config from "../../config";

export interface AuthMissing {
  type: "missing";
}

export interface AuthLoading {
  type: "loading";
}

export interface AuthPresent {
  type: "present";
  accessToken: string;
  validUntil: Date;
  userId: string;
  userLogin: string;
  userName: string;
  userProfileImageUrl: string;
  userDetailsValidUntil: Date;
  userDetailsValidating: boolean;
}

export type AuthState = AuthMissing | AuthLoading | AuthPresent;

let authState = $state<AuthState>({ type: "missing" });
let loadedFromStorage = false;

export const auth = {
  get current(): AuthState {
    return authState;
  },
};

function parseAuthResponse(json: Record<string, unknown>): AuthPresent {
  return {
    type: "present",
    accessToken: String(json["access_token"]),
    validUntil: new Date(String(json["valid_until"])),
    userId: String(json["user_id"]),
    userLogin: String(json["user_login"]),
    userName: String(json["user_name"]),
    userProfileImageUrl: String(json["user_profile_image_url"]),
    userDetailsValidUntil: new Date(String(json["user_details_valid_until"])),
    userDetailsValidating: false,
  };
}

export function updateAuthState(newAuthState?: AuthState): void {
  let nextState = newAuthState ?? authState;

  if (nextState.type === "present") {
    if (nextState.validUntil < new Date()) {
      // token is irrecoverably expired
      nextState = { type: "missing" };
    } else {
      // if this is expired, then it's time for the backend to re-check that the user has not disconnected
      // the Twitch connection/integration.
      const shouldRevalidate =
        nextState.userDetailsValidUntil.getTime() < Date.now();

      nextState = {
        ...nextState,
        userDetailsValidating: shouldRevalidate,
      };

      if (shouldRevalidate) {
        void revalidateLogin(nextState);
      }
    }
  }

  if (
    typeof window !== "undefined" &&
    (nextState.type === "present" || nextState.type === "missing")
  ) {
    window.localStorage.setItem("auth", JSON.stringify(nextState));
  }

  authState = nextState;
}

export function loadStoredAuth(): void {
  if (loadedFromStorage || typeof window === "undefined") {
    return;
  }
  loadedFromStorage = true;

  const storedAuthRaw = window.localStorage.getItem("auth");
  if (storedAuthRaw != null) {
    try {
      const storedAuth: AuthState = JSON.parse(storedAuthRaw);
      if (storedAuth && storedAuth.type === "present") {
        storedAuth.validUntil = new Date(storedAuth.validUntil);
        storedAuth.userDetailsValidUntil = new Date(
          storedAuth.userDetailsValidUntil,
        );
        updateAuthState(storedAuth);
      } else if (storedAuth && storedAuth.type === "missing") {
        updateAuthState(storedAuth);
      }
    } catch (err) {
      console.error("Failed to parse stored auth state", err);
      window.localStorage.removeItem("auth");
    }
  }
}

export function initAuth(): () => void {
  loadStoredAuth();

  const ticker = setInterval(
    () => {
      updateAuthState();
    },
    3 * 60 * 1000,
  ); // every 3 minutes

  return () => {
    clearInterval(ticker);
  };
}

export async function revalidateLogin(currentAuth: AuthPresent): Promise<void> {
  try {
    const response = await fetch(`${config.api_base_url}/auth/extend`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${currentAuth.accessToken}`,
        Accept: "application/json",
      },
    });
    if (!response.ok) {
      throw new Error(response.statusText);
    }
    const json = (await response.json()) as Record<string, unknown>;
    updateAuthState(parseAuthResponse(json));
  } catch (err) {
    console.error("API Request to extend/revalidate authorization failed", err);
    updateAuthState({ type: "missing" });
  }
}

export async function createAuth(code: string): Promise<AuthPresent> {
  const response = await fetch(
    `${config.api_base_url}/auth/create?code=${encodeURIComponent(code)}`,
    {
      method: "POST",
      headers: {
        Accept: "application/json",
      },
    },
  );
  if (!response.ok) {
    throw new Error(response.statusText);
  }
  const json = (await response.json()) as Record<string, unknown>;
  return parseAuthResponse(json);
}

export async function revokeAuth(accessToken: string): Promise<void> {
  const response = await fetch(`${config.api_base_url}/auth/revoke`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${accessToken}`,
      Accept: "application/json",
    },
  });
  if (!response.ok) {
    throw new Error(response.statusText);
  }
}

export function randomHex(byteLength: number): string {
  const bytes = window.crypto.getRandomValues(new Uint8Array(byteLength));
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

export function sanitizeReturnTo(raw: string | null): string {
  if (
    raw &&
    raw.startsWith("/") &&
    !raw.startsWith("//") &&
    !raw.startsWith("/\\")
  ) {
    return raw;
  }
  return "/";
}
