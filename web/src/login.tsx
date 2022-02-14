import arrayBufferToHex from "array-buffer-to-hex";
import * as qs from "qs";
import * as React from "react";
import { Link, Navigate, useLocation, Location } from "react-router-dom";
import { Alert, Button, Spinner } from "reactstrap";
import config from "../config";
import { AuthState, AuthPresent } from "./index";

class Login extends React.Component<
  {
    updateAuthState: (newAuthState: AuthState) => void;
    location: Location;
  },
  {}
> {
  componentDidMount() {
    let randomBytes = window.crypto.getRandomValues(new Uint8Array(32)).buffer; // 256 bits of entropy (32 * 8 bits)
    let csrfToken = arrayBufferToHex(randomBytes);

    let returnTo = qs.parse(this.props.location.search, {
      ignoreQueryPrefix: true,
    }).returnTo;
    if (typeof returnTo !== "string") {
      returnTo = "/";
    }

    window.sessionStorage.setItem(
      "csrfState",
      JSON.stringify({
        token: csrfToken,
        expires: Date.now() + 10 * 60 * 1000, // 10 minutes
        returnTo,
      })
    );

    this.props.updateAuthState({ type: "loading" });

    let authorizeUrl = `https://id.twitch.tv/oauth2/authorize?client_id=${encodeURIComponent(
      config.client_id
    )}&redirect_uri=${encodeURIComponent(
      config.redirect_uri
    )}&response_type=code&scope=&state=${encodeURIComponent(csrfToken)}`;
    window.location.replace(authorizeUrl);
  }

  componentWillUnmount() {
    this.props.updateAuthState({ type: "missing" });
  }

  render() {
    return (
      <Alert fade={false} color="primary">
        <h4 className="alert-heading">
          <Spinner color="primary" className="mr-3" />
          Logging in...
        </h4>
        Sending you to Twitch...
      </Alert>
    );
  }
}

export function LoginWithRouter(updateAuthState: (newAuthState: AuthState) => void) {
  let location = useLocation();
  return <Login updateAuthState={updateAuthState} location={location} />;
}

type AuthorizedComponentState =
  | { type: "error"; message: string; returnTo: string }
  | { type: "loadToken"; code: string; returnTo: string }
  | { type: "finished"; returnTo: string };

type AuthorizedComponentProps = {
  updateAuthState: (newAuthState: AuthState) => void;
  location: Location;
};

class Authorized extends React.Component<
  AuthorizedComponentProps,
  AuthorizedComponentState
> {
  constructor(props: AuthorizedComponentProps) {
    super(props);
    this.state = this.parseResponse();
  }

  // parse all the various bits of information from props and state into an object describing what should be done.
  parseResponse(): AuthorizedComponentState {
    let ownCsrfStateRaw = window.sessionStorage.getItem("csrfState");
    window.sessionStorage.removeItem("csrfState");
    if (ownCsrfStateRaw == null) {
      return {
        type: "error",
        message: "No CSRF token found in browser storage",
        returnTo: "/",
      };
    }

    let ownCsrfState = JSON.parse(ownCsrfStateRaw);
    let { token: expectedCsrfToken, returnTo, expires } = ownCsrfState;
    if (Date.now() > expires) {
      return {
        type: "error",
        message:
          "Login attempt expired. (You took too long to complete the login)",
        returnTo,
      };
    }

    let queryString = qs.parse(this.props.location.search, {
      ignoreQueryPrefix: true,
    });

    let realCsrfToken = queryString["state"];
    if (typeof realCsrfToken !== "string") {
      return {
        type: "error",
        message: "State parameter not correctly present on request",
        returnTo,
      };
    }

    if (realCsrfToken !== expectedCsrfToken) {
      return {
        type: "error",
        message: "CSRF tokens do not match",
        returnTo,
      };
    }

    let errorCode = queryString.error;
    if (errorCode != null) {
      if (errorCode === "access_denied") {
        // User pressed cancel, don't show them an error, just return them to where they came from.
        return {
          type: "finished",
          returnTo,
        };
      } else {
        let errorMessage =
          "Authorization completed with error code " + errorCode;
        if (typeof queryString.error_description === "string") {
          errorMessage += ` (Description: ${queryString.error_description})`;
        }

        return {
          type: "error",
          message: errorMessage,
          returnTo,
        };
      }
    }

    let code = queryString.code;
    if (typeof code !== "string") {
      return {
        type: "error",
        message: "Missing code parameter, or not correctly specified",
        returnTo,
      };
    }

    // All is ok. Once the component mounts, start the API request to load the token.
    return {
      type: "loadToken",
      code,
      returnTo,
    };
  }

  componentDidMount() {
    if (this.state.type !== "loadToken") {
      return;
    }
    let code = this.state.code;

    (async () => {
      try {
        const response = await fetch(
          `${config.api_base_url}/auth/create?code=${encodeURIComponent(code)}`,
          {
            method: "POST",
            headers: {
              Accept: "application/json",
            },
          }
        );
        const json = await response.json();

        let newAuthState: AuthPresent = {
          type: "present",
          accessToken: json["access_token"],
          validUntil: new Date(json["valid_until"]),
          userId: json["user_id"],
          userLogin: json["user_login"],
          userName: json["user_name"],
          userProfileImageUrl: json["user_profile_image_url"],
          userDetailsValidUntil: new Date(json["user_details_valid_until"]),
          userDetailsValidating: false,
        };
        this.props.updateAuthState(newAuthState);

        this.setState((state) => {
          return {
            type: "finished",
            returnTo: state.returnTo,
          };
        });
      } catch (err) {
        console.error("API Request to create authorization failed", err);
        this.setState((state) => {
          return {
            type: "error",
            message: "API Request to create authorization failed",
            returnTo: state.returnTo,
          };
        });

        this.props.updateAuthState({ type: "missing" });
      }
    })();

    this.props.updateAuthState({ type: "loading" });
  }

  render() {
    switch (this.state.type) {
      case "loadToken":
        return (
          <Alert fade={false} color="primary">
            <h4 className="alert-heading">
              <Spinner color="primary" className="mr-3" />
              Logging in...
            </h4>
            Completing login...
          </Alert>
        );
      case "error":
        return (
          <Alert fade={false} color="danger">
            <h4 className="alert-heading">Failed to log you in!</h4>
            There was an unexpected error while trying to log you in. (Technical
            error details: {this.state.message})
            <hr />
            Click below to go back to where you came from.
            <br />
            <Link to={this.state.returnTo}>
              <Button color="primary">Go back</Button>
            </Link>
          </Alert>
        );
      case "finished":
        return <Navigate to={this.state.returnTo} />;
    }
  }
}

export function AuthorizedWithRouter(updateAuthState: (newAuthState: AuthState) => void) {
  const location = useLocation();
  return <Authorized updateAuthState={updateAuthState} location={location} />
}

export function Logout(props: {
  auth: AuthState;
  updateAuthState: (newAuthState: AuthState) => void;
}) {
  React.useEffect(() => {
    if (props.auth.type === "present") {
      fetch(`${config.api_base_url}/auth/revoke`, {
        method: "POST",
        headers: {
          Authorization: `Bearer ${props.auth.accessToken}`,

          Accept: "application/json",
        },
      })
        .then(() => {
          console.log("Successfully finished revoking token");
        })
        .catch((e) => {
          console.error("Failed to revoke login token: ", e);
        });
    }

    props.updateAuthState({ type: "missing" });
  }, []);

  let location = useLocation();
  let returnTo = qs.parse(location.search, {
    ignoreQueryPrefix: true,
  }).returnTo;
  if (typeof returnTo !== "string") {
    returnTo = "/";
  }

  return <Navigate to={returnTo} />;
}

export function revalidateLogin(
  authState: AuthPresent,
  updateAuthState: (newAuthState: AuthState) => void
) {
  // this is called when "userDetailsValidUntil" runs out on the token. the backend then checks whether
  // the Twitch auth connection is still active, and possibly also updates user details like name/profile image.
  // -> the backend returns us a new token with an extended "userDetailsValidUntil" and a fresh "validUntil"
  console.log("Revalidating authentication");

  (async () => {
    try {
      const resp = await fetch(`${config.api_base_url}/auth/extend`, {
        method: "POST",
        headers: {
          Authorization: `Bearer ${authState.accessToken}`,
          Accept: "application/json",
        },
      });
      const json = await resp.json();

      let newAuthState: AuthPresent = {
        type: "present",
        accessToken: json["access_token"],
        validUntil: new Date(json["valid_until"]),
        userId: json["user_id"],
        userLogin: json["user_login"],
        userName: json["user_name"],
        userProfileImageUrl: json["user_profile_image_url"],
        userDetailsValidUntil: new Date(json["user_details_valid_until"]),
        userDetailsValidating: false,
      };
      updateAuthState(newAuthState);
    } catch (err) {
      console.error(
        "API Request to extend/revalidate authorization failed",
        err
      );
      updateAuthState({ type: "missing" });
    }
  })();
}
