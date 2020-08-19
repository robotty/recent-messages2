import axios from "axios";
import React from "react";
import { Link } from "react-router-dom";
import {
  Alert,
  Button,
  CustomInput,
  Form,
  FormGroup,
  Row,
  Spinner,
  Tooltip,
} from "reactstrap";
import * as config from "../config";
import { AuthPresent, AuthState } from "./index";

class SettingsLoggedIn extends React.Component<
  { auth: AuthPresent; updateAuthState: (newAuthState: AuthState) => void },
  {
    ignored: boolean;
    loadingIgnored: boolean;
    loadingIgnoredFailed: boolean;
    savingIgnored: boolean;
    savingIgnoredSuccess: boolean;
    savingIgnoredFailed: boolean;
    currentlyPurging: boolean;
    purgeSuccess: boolean;
    purgeFailed: boolean;
    purgeButtonTooltipOpen: boolean;
  }
> {
  constructor(props) {
    super(props);
    this.state = {
      ignored: false,
      loadingIgnored: false,
      loadingIgnoredFailed: false,
      savingIgnored: false,
      savingIgnoredSuccess: false,
      savingIgnoredFailed: false,
      currentlyPurging: false,
      purgeSuccess: false,
      purgeFailed: false,
      purgeButtonTooltipOpen: false,
    };
    this.updateIgnored = this.updateIgnored.bind(this);
    this.purgeMessages = this.purgeMessages.bind(this);
    this.togglePurgeButtonTooltip = this.togglePurgeButtonTooltip.bind(this);
  }

  componentDidMount() {
    this.setState(() => {
      return {
        loadingIgnored: true,
        loadingIgnoredFailed: false,
      };
    });

    axios
      .get(`${config.api_base_url}/ignored`, {
        headers: {
          Authorization: `Bearer ${this.props.auth.accessToken}`,
        },
      })
      .then((resp) => {
        this.setState(() => {
          return {
            ignored: resp.data["ignored"],
            loadingIgnored: false,
            loadingIgnoredFailed: false,
          };
        });
      })
      .catch((err) => {
        console.error("Failed to load `ignored` status of channel", err);
        this.setState(() => {
          return {
            loadingIgnored: false,
            loadingIgnoredFailed: true,
          };
        });
      });
  }

  updateIgnored(e) {
    let previousSetting = this.state.ignored;
    let newSetting = e.target.checked;

    this.setState(() => {
      return {
        ignored: newSetting,
        savingIgnored: true,
        savingIgnoredFailed: false,
        savingIgnoredSuccess: false,
        purgeSuccess: false,
        purgeFailed: false,
      };
    });

    axios
      .post(
        `${config.api_base_url}/ignored`,
        { ignored: newSetting },
        {
          headers: {
            Authorization: `Bearer ${this.props.auth.accessToken}`,
          },
        }
      )
      .then((resp) => {
        this.setState(() => {
          return {
            savingIgnored: false,
            savingIgnoredSuccess: true,
          };
        });
      })
      .catch((err) => {
        console.error("Failed to load `ignored` status of channel", err);
        this.setState(() => {
          return {
            ignored: previousSetting,
            savingIgnored: false,
            loadingIgnoredFailed: true,
          };
        });
      });
  }

  purgeMessages() {
    console.log("User clicked purge button");

    this.setState(() => {
      return {
        currentlyPurging: true,
        savingIgnoredFailed: false,
        savingIgnoredSuccess: false,
        purgeFailed: false,
        purgeSuccess: false,
      };
    });

    axios
      .post(`${config.api_base_url}/purge`, undefined, {
        headers: {
          Authorization: `Bearer ${this.props.auth.accessToken}`,
        },
      })
      .then((resp) => {
        this.setState(() => {
          return {
            currentlyPurging: false,
            purgeSuccess: true,
          };
        });
      })
      .catch((err) => {
        console.error("Failed to purge messages in channel", err);
        this.setState(() => {
          return {
            currentlyPurging: false,
            purgeFailed: true,
          };
        });
      });
  }

  togglePurgeButtonTooltip() {
    this.setState((state) => {
      return {
        purgeButtonTooltipOpen: !state.purgeButtonTooltipOpen,
      };
    });
  }

  render() {
    return (
      <>
        <section>
          <p>
            You can make the following privacy settings for your own channel:
          </p>
        </section>
        <section>
          <Form>
            <FormGroup>
              <div>
                <CustomInput
                  inline
                  type="switch"
                  id="ignored"
                  label="Blacklist my channel"
                  disabled={
                    this.state.savingIgnored ||
                    this.state.loadingIgnored ||
                    this.state.loadingIgnoredFailed
                  }
                  checked={this.state.ignored}
                  onChange={this.updateIgnored}
                  aria-describedby="ignored-help-block"
                />
                {(this.state.savingIgnored || this.state.loadingIgnored) && (
                  <Spinner className="mr-2" size="sm" color="primary" />
                )}
                {this.state.savingIgnored && (
                  <span className="text-primary">Saving changes…</span>
                )}
                {this.state.loadingIgnored && (
                  <span className="text-primary">Loading settings…</span>
                )}

                {this.state.savingIgnoredSuccess && (
                  <span className="text-success">
                    <i className="fas fa-check mr-1" /> Saved.
                  </span>
                )}
                {this.state.savingIgnoredFailed && (
                  <span className="text-danger">
                    <i className="fas fa-times mr-1" /> Failed to save. Please
                    try again.
                  </span>
                )}
                {this.state.loadingIgnoredFailed && (
                  <span className="text-danger">
                    <i className="fas fa-times mr-1" /> Failed to load current
                    setting. Please try again.
                  </span>
                )}
              </div>
              <small className="form-text text-muted" id="ignored-help-block">
                Removes your channel entirely from the service. No messages will
                be recorded and nobody will be able to load any recent messages
                for your channel. Additionally, messages currently stored for
                your channel will be deleted.
              </small>
            </FormGroup>
            <FormGroup>
              <div>
                <span className="d-inline-block" id="purge-button">
                  <Button
                    color="danger"
                    aria-describedby="purge-help-block"
                    onClick={this.purgeMessages}
                    disabled={this.state.ignored || this.state.currentlyPurging}
                    style={this.state.ignored ? { pointerEvents: "none" } : {}}
                  >
                    Purge messages
                  </Button>
                </span>
                {this.state.ignored && (
                  <Tooltip
                    placement="bottom"
                    isOpen={this.state.purgeButtonTooltipOpen}
                    target="purge-button"
                    toggle={this.togglePurgeButtonTooltip}
                  >
                    You cannot purge messages for your channel because you have
                    set your channel to be blacklisted. Since your channel is
                    blacklisted, no messages are stored for it.
                  </Tooltip>
                )}
                {this.state.currentlyPurging && (
                  <span className="ml-3 text-primary">
                    <Spinner className="mr-2" size="sm" color="primary" />
                    Purging messages…
                  </span>
                )}
                {this.state.purgeSuccess && (
                  <span className="ml-3 text-success">
                    <i className="fas fa-check mr-1" /> Messages have been
                    purged.
                  </span>
                )}
                {this.state.purgeFailed && (
                  <span className="ml-3 text-danger">
                    <i className="fas fa-times mr-1" /> Failed to purge
                    messages. Please try again.
                  </span>
                )}
              </div>
              <small className="form-text text-muted" id="purge-help-block">
                Remove all messages currently stored for your channel. This is a
                one-time action. Purging messages cannot be undone.
              </small>
            </FormGroup>
          </Form>
        </section>
      </>
    );
  }
}

export function Settings(props: {
  auth: AuthState;
  updateAuthState: (newAuthState: AuthState) => void;
}) {
  if (props.auth.type === "present") {
    return (
      <SettingsLoggedIn
        auth={props.auth}
        updateAuthState={props.updateAuthState}
      />
    );
  } else {
    return (
      <Alert fade={false} color="warning">
        <h4 className="alert-heading">Not logged in</h4>
        You are currently not logged in. Use the button below or the link on the
        navigation bar to log in.
        <br />
        <Link to="/login?returnTo=%2Fsettings">
          <Button color="primary">
            <i className="fas fa-sign-in-alt mr-1" />
            Log in
          </Button>
        </Link>
      </Alert>
    );
  }
}
