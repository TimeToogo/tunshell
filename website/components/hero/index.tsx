import React, { useEffect, useState } from "react";
import { Container } from "../layout";
import * as Styled from "./styled";

const allSteps = [
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

const TypingText = ({ steps }: { steps: string[] }) => {
  const [stepIndex, setStepIndex] = useState(0);
  const [text, setText] = useState("");
  const [isDeleting, setIsDeleting] = useState(false);

  useEffect(() => {
    if (!steps.length) {
      return;
    }

    const currentStep = steps[stepIndex % steps.length];

    if (!isDeleting && text === currentStep) {
      const timeout = window.setTimeout(() => setIsDeleting(true), 2000);
      return () => window.clearTimeout(timeout);
    }

    if (isDeleting && text.length === 0) {
      setIsDeleting(false);
      setStepIndex((currentIndex) => (currentIndex + 1) % steps.length);
      return;
    }

    const timeout = window.setTimeout(() => {
      setText((currentText) =>
        isDeleting ? currentText.slice(0, -1) : currentStep.slice(0, currentText.length + 1),
      );
    }, isDeleting ? 40 : 80);

    return () => window.clearTimeout(timeout);
  }, [isDeleting, stepIndex, steps, text]);

  return <span className="typing">{text}</span>;
};

export const Hero = () => {
  const [steps, setSteps] = useState(allSteps);

  useEffect(() => {
    const setStepList = () => {
      setSteps(window.innerWidth <= 768 ? allSteps.filter((step) => step.length <= 14) : allSteps);
    };

    setStepList();
    window.addEventListener("resize", setStepList);

    return () => window.removeEventListener("resize", setStepList);
  }, []);

  return (
    <Styled.Hero>
      <Container>
        <h1>
          Rapidly shell into <TypingText steps={steps} />
        </h1>
        <p>
          Tunshell is a simple and secure method to remote shell into ephemeral environments such as deployment
          pipelines or serverless functions
        </p>
      </Container>
    </Styled.Hero>
  );
};
