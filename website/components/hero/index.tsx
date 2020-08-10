import React from "react";
import { Container } from "../layout";
import * as Styled from "./styled";
import Typical from "react-typical";
import { InstallScriptService, InstallScriptType } from "../../services/install-script";

export const Hero = () => {
  return (
    <Styled.Hero>
      <Container>
        <h1>
          Rapidly shell into{" "}
          <Typical
            className="typing"
            steps={[
              "remote hosts",
              2000,
              "Unix",
              2000,
              "Windows",
              2000,
              "Jenkins",
              2000,
              "AWS Lambda",
              2000,
              "BitBucket Pipelines",
              2000,
              "Cloud Build",
              2000,
              "GitHub Actions",
              2000,
              "Cloud Functions",
              2000,
              "GitLab CI",
              2000,
              "remote hosts",
            ]}
            wrapper="span"
          />
        </h1>
        <p>Tunshell is a simple and secure method to remote shell into ephemeral environments such as deployment pipelines or serverless functions</p>
      </Container>
    </Styled.Hero>
  );
};
