import * as React from "react";
import * as ReactDOM from "react-dom";
import { BrowserRouter, Outlet, Route, Routes } from "react-router-dom";
import { Container } from "reactstrap";
import {
  AuthorizedWithRouter,
  LoginWithRouter,
  Logout,
  revalidateLogin,
} from "./login";
import { NavWithRouter } from "./nav";
import { Settings } from "./settings";
import { API, DonationThankYou, Home, Privacy } from "./static";

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

export class App extends React.Component<{}, { auth: AuthState }> {
  private runningTicker: ReturnType<typeof setTimeout> | undefined;

  constructor(props: {}) {
    super(props);
    this.state = {
      auth: {
        type: "missing",
      },
    };
    this.runningTicker = undefined;

    this.updateAuthState = this.updateAuthState.bind(this);
  }

  updateAuthState(newAuthState?: AuthState) {
    this.setState((oldState) => {
      // If newAuthState is present we check it for expiration first, and then actually apply a state based on that.
      // If newAuthState is undefined we simply check the existing state for expiration instead of a (different) new one.
      if (newAuthState == null) {
        newAuthState = oldState.auth;
      }

      console.log("got request to update auth state to:", newAuthState);
      if (newAuthState.type === "present") {
        if (newAuthState.validUntil < new Date()) {
          // token is irrecoverably expired
          newAuthState = { type: "missing" };
        } else {
          // if this is expired, then it's time for the backend to re-check that the user has not disconnected
          // the Twitch connection/integration.

          // note because userDetailsValidUntil (soft expiry) is always expected to be earlier than validUntil (hard expiry)
          // we only check whether we are close to that (lower) expiry - the revalidateLogin method below
          // extends both validity periods at the same time
          let shouldRevalidate =
            newAuthState.userDetailsValidUntil.getTime() < Date.now();

          newAuthState = {
            ...newAuthState,
            userDetailsValidating: shouldRevalidate,
          };

          if (shouldRevalidate) {
            revalidateLogin(newAuthState, this.updateAuthState);
          }
        }
      }
      console.log(
        "after checking expirations, new auth state is actually going to be:",
        newAuthState
      );

      if (newAuthState.type === "present" || newAuthState.type === "missing") {
        console.log("updated auth in localStorage");
        window.localStorage.setItem("auth", JSON.stringify(newAuthState));
      }

      return { auth: newAuthState };
    });
  }

  componentDidMount() {
    let storedAuthRaw = window.localStorage.getItem("auth");
    if (storedAuthRaw != null) {
      console.log("loaded auth from localStorage");
      let storedAuth: AuthState = JSON.parse(storedAuthRaw);
      if (storedAuth.type === "present") {
        storedAuth.validUntil = new Date(storedAuth.validUntil);
        storedAuth.userDetailsValidUntil = new Date(
          storedAuth.userDetailsValidUntil
        );
      }
      this.updateAuthState(storedAuth);
    }

    this.runningTicker = setInterval(() => {
      console.log("tick! making sure auth has not expired");
      this.updateAuthState();
    }, 3 * 60 * 1000); // every 3 minutes
  }

  componentWillUnmount() {
    if (this.runningTicker != null) {
      clearInterval(this.runningTicker);
    }
  }

  render() {
    let navAndContainer = (
      <>
        <NavWithRouter auth={this.state.auth} />
        <Container className="pt-3">
          <Outlet />
        </Container>
      </>
    );
    return (
      <BrowserRouter>
        <Routes>
          <Route path="/" element={navAndContainer}>
            <Route path="/api" element={<API />} />
            <Route
              path="/settings"
              element={
                <Settings
                  auth={this.state.auth}
                  updateAuthState={this.updateAuthState}
                />
              }
            />
            <Route
              path="/login"
              element={
                <LoginWithRouter updateAuthState={this.updateAuthState} />
              }
            />
            <Route
              path="/authorized"
              element={
                <AuthorizedWithRouter updateAuthState={this.updateAuthState} />
              }
            />
            <Route
              path="/logout"
              element={
                <Logout
                  auth={this.state.auth}
                  updateAuthState={this.updateAuthState}
                />
              }
            />
            <Route path="/privacy" element={<Privacy />} />
            <Route path="/donation-thank-you" element={<DonationThankYou />} />
            <Route path="/" element={<Home />} />
            <Route
              path="*"
              element={
                <>
                  <h1>Not Found</h1>
                  The page you were trying to access does not exist.
                </>
              }
            />
          </Route>
        </Routes>
      </BrowserRouter>
    );
  }
}

ReactDOM.render(<App />, document.getElementById("root"));
