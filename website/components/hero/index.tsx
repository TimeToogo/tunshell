import React from "react";
import { Container } from "../layout";
import * as Styled from "./styled";
import Typical from "react-typical";

export const Hero = () => {
  let steps = [
    "remote hosts",
    "Unix",
    "Windows",
    "Jenkins",
    "AWS Lambda",
    "BitBucket Pipelines",
    "Cloud Build",
    "GitHub Actions",
    "Cloud Functions",
    "GitLab CI",
    "remote hosts",
  ];

  if (typeof window !== "undefined" && window.innerWidth <= 768) {
    steps = steps.filter((i) => i.length <= 14);
  }

  return (
    <Styled.Hero>
      <Container>
        <h1>
          Rapidly shell into{" "}
          <Typical className="typing" steps={steps.reduce((a, i) => a.concat([i, 2000]), [])} wrapper="span" />
        </h1>
        <p>
          Tunshell is a simple and secure method to remote shell into ephemeral environments such as deployment
          pipelines or serverless functions
        </p>
      </Container>
    </Styled.Hero>
  );
};
